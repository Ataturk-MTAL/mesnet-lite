//! İşletme gruplama (Issue #42): atama panosunda atanmamış işletmeleri
//! kümeler. Saf alan kodudur; veritabanına dokunmaz.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::domain::address::normalize_place_name;
use crate::error::{AppError, AppResult};

pub const MODE_KEY: &str = "grouping_mode";
pub const MAX_DIAMETER_KEY: &str = "grouping_max_diameter_km";
pub const MANUAL_GROUPS_KEY: &str = "grouping_manual_groups";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingMode {
    Distance,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ManualGroup {
    pub name: String,
    #[serde(default)]
    pub neighborhoods: Vec<String>,
    #[serde(default)]
    pub districts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupingSettings {
    pub mode: GroupingMode,
    pub max_diameter_km: f64,
    pub manual_groups: Vec<ManualGroup>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupingInput {
    pub company_id: i64,
    pub neighborhood: String,
    pub district: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyGroup {
    pub key: String,
    pub label: String,
}

/// Varsayılan çap: gerçek veride 3 km, complete-linkage ile makul kümeler
/// verdi (6,4,3,2,1×6); "en yakın komşuya bağla" 2 km'de bile zincirlendi.
pub const DEFAULT_MAX_DIAMETER_KM: f64 = 3.0;
pub const MIN_DIAMETER_KM: f64 = 0.5;
pub const MAX_DIAMETER_KM: f64 = 50.0;
pub const GROUP_NAME_MAX_CHARS: usize = 80;
/// Etikette adı sayılacak en çok mahalle/ilçe; fazlası `+N` ile özetlenir.
const LABEL_NAME_LIMIT: usize = 2;
/// Dünya'nın ortalama yarıçapı (km) — haversine için.
const EARTH_RADIUS_KM: f64 = 6371.0;

impl GroupingSettings {
    /// Kayıtlı değer eksik ya da bozuksa varsayılana düşer; ayar okuması
    /// panoyu asla kırmamalı (kaydetme anındaki doğrulama bozuk değeri zaten
    /// engeller, ama elle değiştirilmiş bir veritabanı olabilir).
    pub fn from_settings(settings: &BTreeMap<String, String>) -> GroupingSettings {
        GroupingSettings {
            mode: settings
                .get(MODE_KEY)
                .and_then(|v| parse_mode(v))
                .unwrap_or(GroupingMode::Distance),
            max_diameter_km: settings
                .get(MAX_DIAMETER_KEY)
                .and_then(|v| parse_diameter(v))
                .unwrap_or(DEFAULT_MAX_DIAMETER_KM),
            manual_groups: settings
                .get(MANUAL_GROUPS_KEY)
                .and_then(|v| parse_manual_groups(v).ok())
                .unwrap_or_default(),
        }
    }
}

fn parse_mode(raw: &str) -> Option<GroupingMode> {
    match raw {
        "distance" => Some(GroupingMode::Distance),
        "manual" => Some(GroupingMode::Manual),
        _ => None,
    }
}

fn parse_diameter(raw: &str) -> Option<f64> {
    let value: f64 = raw.trim().parse().ok()?;
    (MIN_DIAMETER_KM..=MAX_DIAMETER_KM).contains(&value).then_some(value)
}

fn parse_manual_groups(raw: &str) -> Result<Vec<ManualGroup>, serde_json::Error> {
    serde_json::from_str(raw)
}

/// Gruplama ayarlarını kaydetmeden önce doğrular; yalnız gönderilen anahtarlar
/// denetlenir (`commission_minutes::validate_minutes_settings` ile aynı desen).
pub fn validate_settings(entries: &BTreeMap<String, String>) -> AppResult<()> {
    if let Some(mode) = entries.get(MODE_KEY) {
        if parse_mode(mode).is_none() {
            return Err(AppError::Validation(
                "Gruplama modu \"distance\" (mesafe) ya da \"manual\" (elle) olmalı.".into(),
            ));
        }
    }
    if let Some(diameter) = entries.get(MAX_DIAMETER_KEY) {
        if parse_diameter(diameter).is_none() {
            return Err(AppError::Validation(format!(
                "Küme çapı {MIN_DIAMETER_KM} ile {MAX_DIAMETER_KM} km arasında bir sayı olmalı."
            )));
        }
    }
    if let Some(raw) = entries.get(MANUAL_GROUPS_KEY) {
        validate_manual_groups(raw)?;
    }
    Ok(())
}

fn validate_manual_groups(raw: &str) -> AppResult<()> {
    let groups = parse_manual_groups(raw).map_err(|e| {
        AppError::Validation(format!(
            "Elle grup listesi geçersiz: her grup ad, mahalleler ve ilçeler (metin listeleri) içermeli ({e})."
        ))
    })?;
    let mut seen = std::collections::BTreeSet::new();
    for group in &groups {
        let name = group.name.trim();
        if name.is_empty() {
            return Err(AppError::Validation("Grup adı boş olamaz. Lütfen bir ad yazın.".into()));
        }
        if name.chars().count() > GROUP_NAME_MAX_CHARS {
            return Err(AppError::Validation(format!(
                "Grup adı en fazla {GROUP_NAME_MAX_CHARS} karakter olabilir: \"{name}\". Lütfen kısaltın."
            )));
        }
        // Büyük/küçük harf duyarsızlık için yer adı normalleştirmesi (İ/ı doğru).
        if !seen.insert(normalize_place_name(name)) {
            return Err(AppError::Validation(format!(
                "\"{name}\" adı birden fazla grupta kullanılmış. Grup adları benzersiz olmalı."
            )));
        }
    }
    Ok(())
}

/// Her işletmeye bir grup atar; grubu olmayan işletme haritada yoktur.
/// Sonuç girdi sırasından bağımsızdır.
pub fn assign_groups(
    inputs: &[GroupingInput],
    settings: &GroupingSettings,
) -> BTreeMap<i64, CompanyGroup> {
    let mut result = BTreeMap::new();
    let mut remaining: Vec<&GroupingInput> = inputs.iter().collect();
    remaining.sort_by_key(|i| i.company_id);

    match settings.mode {
        GroupingMode::Distance => {
            let (located, unlocated): (Vec<_>, Vec<_>) =
                remaining.into_iter().partition(|i| position(i).is_some());
            result.extend(cluster_by_distance(&located, settings.max_diameter_km));
            remaining = unlocated;
        }
        GroupingMode::Manual => {
            remaining.retain(|i| match match_manual(i, &settings.manual_groups) {
                Some(group) => {
                    result.insert(i.company_id, group);
                    false
                }
                None => true,
            });
        }
    }
    for input in remaining {
        if let Some(group) = district_group(input) {
            result.insert(input.company_id, group);
        }
    }
    result
}

fn position(input: &GroupingInput) -> Option<(f64, f64)> {
    match (input.latitude, input.longitude) {
        (Some(lat), Some(lon)) if lat.is_finite() && lon.is_finite() => Some((lat, lon)),
        _ => None,
    }
}

fn district_group(input: &GroupingInput) -> Option<CompanyGroup> {
    let district = normalize_place_name(&input.district);
    if district.is_empty() {
        return None;
    }
    Some(CompanyGroup { key: format!("district:{district}"), label: district })
}

/// Önce mahalle (listede ilk bulunan grup), sonra ilçe eşleşmesi.
fn match_manual(input: &GroupingInput, groups: &[ManualGroup]) -> Option<CompanyGroup> {
    let find = |value: &str, pick: fn(&ManualGroup) -> &Vec<String>| {
        let wanted = normalize_place_name(value);
        if wanted.is_empty() {
            return None;
        }
        groups
            .iter()
            .position(|g| pick(g).iter().any(|n| normalize_place_name(n) == wanted))
    };
    let index = find(&input.neighborhood, |g| &g.neighborhoods)
        .or_else(|| find(&input.district, |g| &g.districts))?;
    Some(CompanyGroup {
        key: format!("manual:{index}"),
        label: groups[index].name.trim().to_string(),
    })
}

/// Haversine büyük daire uzaklığı (km).
fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let d_lat = lat2 - lat1;
    let d_lon = (b.1 - a.1).to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

/// Complete-linkage: iki kümenin uzaklığı üyeleri arasındaki EN BÜYÜK
/// uzaklıktır; böylece küme çapı sınırı aşamaz (single-linkage zincirlenirdi).
/// `located` kimliğe göre sıralı gelir; kümeler `located` indeksleridir.
fn cluster_by_distance(located: &[&GroupingInput], max_km: f64) -> BTreeMap<i64, CompanyGroup> {
    let points: Vec<(f64, f64)> = located.iter().filter_map(|i| position(i)).collect();
    let mut clusters: Vec<Vec<usize>> = (0..located.len()).map(|i| vec![i]).collect();

    while let Some((a, b)) = closest_mergeable_pair(&clusters, &points, located, max_km) {
        let merged = clusters.remove(b);
        clusters[a].extend(merged);
    }

    // `located` kimliğe göre sıralı olduğundan indeks sırası kimlik sırasıdır:
    // kümenin en küçük indeksi en küçük kimliğidir.
    clusters.iter_mut().for_each(|c| c.sort_unstable());
    clusters.sort_by_key(|c| c[0]);

    let mut result = BTreeMap::new();
    for (n, members) in clusters.iter().enumerate() {
        let number = n + 1;
        let group = CompanyGroup {
            key: format!("cluster:{number}"),
            label: cluster_label(members.iter().map(|&m| located[m]), number),
        };
        for &m in members {
            result.insert(located[m].company_id, group.clone());
        }
    }
    result
}

/// Uzaklığı `max_km`'yi aşmayan en yakın küme çifti; eşitlikte
/// (uzaklık, a'nın en küçük kimliği, b'nin en küçük kimliği) artan.
/// Döndürülen dizinler `a < b`dir.
fn closest_mergeable_pair(
    clusters: &[Vec<usize>],
    points: &[(f64, f64)],
    located: &[&GroupingInput],
    max_km: f64,
) -> Option<(usize, usize)> {
    let min_id = |c: &Vec<usize>| c.iter().map(|&m| located[m].company_id).min().unwrap_or(i64::MAX);
    let mut best: Option<((f64, i64, i64), (usize, usize))> = None;
    for a in 0..clusters.len() {
        for b in (a + 1)..clusters.len() {
            let distance = complete_distance(&clusters[a], &clusters[b], points);
            if distance > max_km {
                continue;
            }
            let (id_a, id_b) = (min_id(&clusters[a]), min_id(&clusters[b]));
            let key = (distance, id_a.min(id_b), id_a.max(id_b));
            let better = best.as_ref().is_none_or(|(k, _)| {
                key.0.total_cmp(&k.0).then(key.1.cmp(&k.1)).then(key.2.cmp(&k.2)).is_lt()
            });
            if better {
                best = Some((key, (a, b)));
            }
        }
    }
    best.map(|(_, pair)| pair)
}

fn complete_distance(a: &[usize], b: &[usize], points: &[(f64, f64)]) -> f64 {
    a.iter()
        .flat_map(|&i| b.iter().map(move |&j| haversine_km(points[i], points[j])))
        .fold(0.0, f64::max)
}

/// Küme etiketi: en kalabalık mahalleler (eşitlikte ada göre), yoksa ilçeler,
/// yoksa "Küme N".
fn cluster_label<'a>(
    members: impl Iterator<Item = &'a GroupingInput> + Clone,
    number: usize,
) -> String {
    let neighborhoods = ranked_names(members.clone().map(|i| i.neighborhood.as_str()));
    let ranked = if neighborhoods.is_empty() {
        ranked_names(members.map(|i| i.district.as_str()))
    } else {
        neighborhoods
    };
    if ranked.is_empty() {
        return format!("Küme {number}");
    }
    let shown = ranked[..ranked.len().min(LABEL_NAME_LIMIT)].join(", ");
    match ranked.len().saturating_sub(LABEL_NAME_LIMIT) {
        0 => shown,
        rest => format!("{shown} +{rest}"),
    }
}

fn ranked_names<'a>(raw: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for name in raw.map(normalize_place_name).filter(|n| !n.is_empty()) {
        *counts.entry(name).or_default() += 1;
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.into_iter().map(|(name, _)| name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: i64, nb: &str, district: &str, pos: Option<(f64, f64)>) -> GroupingInput {
        GroupingInput {
            company_id: id,
            neighborhood: nb.into(),
            district: district.into(),
            latitude: pos.map(|p| p.0),
            longitude: pos.map(|p| p.1),
        }
    }

    fn distance_settings(max_km: f64) -> GroupingSettings {
        GroupingSettings { mode: GroupingMode::Distance, max_diameter_km: max_km, manual_groups: vec![] }
    }

    fn manual_group(name: &str, nbs: &[&str], districts: &[&str]) -> ManualGroup {
        ManualGroup {
            name: name.into(),
            neighborhoods: nbs.iter().map(|s| s.to_string()).collect(),
            districts: districts.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn manual_settings(groups: Vec<ManualGroup>) -> GroupingSettings {
        GroupingSettings { mode: GroupingMode::Manual, max_diameter_km: 3.0, manual_groups: groups }
    }

    /// Ekvator üzerinde `km` kadar doğuya giden boylam (derece).
    fn lon_for_km(km: f64) -> f64 {
        km / 111.195
    }

    fn entries(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn keys(map: &BTreeMap<i64, CompanyGroup>, ids: &[i64]) -> Vec<String> {
        ids.iter().map(|id| map[id].key.clone()).collect()
    }

    #[test]
    fn two_near_points_and_one_far_point_make_two_clusters() {
        let inputs = [
            input(1, "A", "X", Some((0.0, 0.0))),
            input(2, "B", "X", Some((0.0, lon_for_km(1.0)))),
            input(3, "C", "X", Some((0.0, lon_for_km(30.0)))),
        ];
        let groups = assign_groups(&inputs, &distance_settings(3.0));
        assert_eq!(groups[&1].key, "cluster:1");
        assert_eq!(groups[&2].key, "cluster:1");
        assert_eq!(groups[&3].key, "cluster:2");
    }

    /// Complete-linkage: zincirin uçları (4 km) çapı aşarsa tek kümede olamaz.
    #[test]
    fn a_chain_exceeding_the_diameter_is_not_one_cluster() {
        let inputs = [
            input(1, "A", "X", Some((0.0, 0.0))),
            input(2, "B", "X", Some((0.0, lon_for_km(2.0)))),
            input(3, "C", "X", Some((0.0, lon_for_km(4.0)))),
        ];
        let groups = assign_groups(&inputs, &distance_settings(3.0));
        let distinct: std::collections::BTreeSet<_> = keys(&groups, &[1, 2, 3]).into_iter().collect();
        assert_eq!(distinct.len(), 2);
    }

    /// AB ve BC tam eşit (0,5 derece, ekvatorda): kimliği küçük çift kazanır.
    #[test]
    fn ties_merge_the_pair_with_the_smaller_ids_first() {
        let inputs = [
            input(1, "A", "X", Some((0.0, 0.0))),
            input(2, "B", "X", Some((0.0, 0.5))),
            input(3, "C", "X", Some((0.0, 1.0))),
        ];
        let groups = assign_groups(&inputs, &distance_settings(60.0));
        assert_eq!(groups[&1].key, groups[&2].key);
        assert_ne!(groups[&2].key, groups[&3].key);
    }

    #[test]
    fn input_order_does_not_change_the_result() {
        let mut inputs = vec![
            input(1, "A", "X", Some((0.0, 0.0))),
            input(2, "B", "X", Some((0.0, 0.5))),
            input(3, "C", "X", Some((0.0, 1.0))),
            input(4, "D", "Y", None),
            input(5, "", "", None),
        ];
        let forward = assign_groups(&inputs, &distance_settings(60.0));
        inputs.reverse();
        assert_eq!(forward, assign_groups(&inputs, &distance_settings(60.0)));
    }

    /// Kümeler en küçük üye kimliğine göre numaralanır, giriş sırasına göre değil.
    #[test]
    fn clusters_are_numbered_by_smallest_member_id() {
        let inputs = [
            input(9, "Z", "X", Some((0.0, 0.0))),
            input(2, "Y", "X", Some((0.0, lon_for_km(50.0)))),
        ];
        let groups = assign_groups(&inputs, &distance_settings(3.0));
        assert_eq!(groups[&2].key, "cluster:1");
        assert_eq!(groups[&9].key, "cluster:2");
    }

    #[test]
    fn label_lists_top_two_neighborhoods_with_remainder_count() {
        let at = Some((0.0, 0.0));
        let inputs = [
            input(1, "Güney", "X", at),
            input(2, "Güney", "X", at),
            input(3, "Kuzey", "X", at),
            input(4, "Batı", "X", at),
            input(5, "Doğu", "X", at),
        ];
        let groups = assign_groups(&inputs, &distance_settings(3.0));
        assert_eq!(groups[&1].label, "Güney, Batı +2");
    }

    #[test]
    fn label_falls_back_to_districts_then_to_numbered_name() {
        let at = Some((0.0, 0.0));
        let by_district = assign_groups(&[input(1, "", "Örnek", at)], &distance_settings(3.0));
        assert_eq!(by_district[&1].label, "Örnek");
        let nameless = assign_groups(&[input(1, "", "", at)], &distance_settings(3.0));
        assert_eq!(nameless[&1].label, "Küme 1");
    }

    #[test]
    fn unlocated_company_falls_to_its_district_group() {
        let inputs = [input(1, "A", "örnek", None), input(2, "B", "ÖRNEK", None)];
        let groups = assign_groups(&inputs, &distance_settings(3.0));
        assert_eq!(groups[&1].key, "district:Örnek");
        assert_eq!(groups[&1], groups[&2]);
        assert_eq!(groups[&1].label, "Örnek");
    }

    #[test]
    fn unlocated_company_without_district_has_no_group() {
        let groups = assign_groups(&[input(1, "A", "  ", None)], &distance_settings(3.0));
        assert!(!groups.contains_key(&1));
    }

    #[test]
    fn manual_matches_neighborhood_before_district() {
        let settings = manual_settings(vec![
            manual_group("İlçe grubu", &[], &["Örnek"]),
            manual_group("Mahalle grubu", &["Deneme"], &[]),
        ]);
        let groups = assign_groups(&[input(1, "Deneme", "Örnek", None)], &settings);
        assert_eq!(groups[&1].key, "manual:1");
        assert_eq!(groups[&1].label, "Mahalle grubu");
    }

    #[test]
    fn manual_first_matching_group_wins() {
        let settings = manual_settings(vec![
            manual_group("Birinci", &["Deneme"], &[]),
            manual_group("İkinci", &["Deneme"], &[]),
        ]);
        let groups = assign_groups(&[input(1, "Deneme", "", None)], &settings);
        assert_eq!(groups[&1].key, "manual:0");
    }

    #[test]
    fn manual_matching_is_turkish_case_insensitive() {
        let settings = manual_settings(vec![manual_group("G", &["ığdır"], &["İZMİR"])]);
        let by_neighborhood = assign_groups(&[input(1, "IĞDIR", "", None)], &settings);
        assert_eq!(by_neighborhood[&1].key, "manual:0");
        let by_district = assign_groups(&[input(2, "", "izmir", None)], &settings);
        assert_eq!(by_district[&2].key, "manual:0");
    }

    #[test]
    fn manual_unmatched_company_falls_to_district_group() {
        let settings = manual_settings(vec![manual_group("G", &["Başka"], &[])]);
        let groups = assign_groups(&[input(1, "Deneme", "Örnek", Some((0.0, 0.0)))], &settings);
        assert_eq!(groups[&1].key, "district:Örnek");
    }

    #[test]
    fn from_settings_defaults_when_missing() {
        let parsed = GroupingSettings::from_settings(&BTreeMap::new());
        assert_eq!(parsed, distance_settings(3.0));
    }

    #[test]
    fn from_settings_falls_back_to_defaults_on_corrupt_values() {
        for bad_diameter in ["abc", "0", "-1", "51", "NaN", "inf", ""] {
            let parsed = GroupingSettings::from_settings(&entries(&[
                (MODE_KEY, "bilinmeyen"),
                (MAX_DIAMETER_KEY, bad_diameter),
                (MANUAL_GROUPS_KEY, "{bozuk"),
            ]));
            assert_eq!(parsed, distance_settings(3.0), "çap: {bad_diameter}");
        }
    }

    #[test]
    fn from_settings_reads_valid_values() {
        let parsed = GroupingSettings::from_settings(&entries(&[
            (MODE_KEY, "manual"),
            (MAX_DIAMETER_KEY, "5.5"),
            (MANUAL_GROUPS_KEY, r#"[{"name":"G","neighborhoods":["A"],"districts":[]}]"#),
        ]));
        assert_eq!(parsed.mode, GroupingMode::Manual);
        assert_eq!(parsed.max_diameter_km, 5.5);
        assert_eq!(parsed.manual_groups, vec![manual_group("G", &["A"], &[])]);
    }

    fn rejects(pairs: &[(&str, &str)]) {
        let result = validate_settings(&entries(pairs));
        assert!(matches!(result, Err(AppError::Validation(_))), "{pairs:?} kabul edildi");
    }

    #[test]
    fn validate_accepts_empty_and_valid_entries() {
        assert!(validate_settings(&entries(&[])).is_ok());
        assert!(validate_settings(&entries(&[
            (MODE_KEY, "manual"),
            (MAX_DIAMETER_KEY, "0.5"),
            (MANUAL_GROUPS_KEY, r#"[{"name":"A","neighborhoods":[],"districts":["X"]},{"name":"B","neighborhoods":[],"districts":[]}]"#),
        ]))
        .is_ok());
    }

    #[test]
    fn validate_rejects_bad_mode() {
        rejects(&[(MODE_KEY, "auto")]);
    }

    #[test]
    fn validate_rejects_bad_diameter() {
        for bad in ["abc", "0.4", "50.1", "NaN", "inf", ""] {
            rejects(&[(MAX_DIAMETER_KEY, bad)]);
        }
    }

    #[test]
    fn validate_rejects_bad_manual_groups() {
        rejects(&[(MANUAL_GROUPS_KEY, "{bozuk")]);
        rejects(&[(MANUAL_GROUPS_KEY, r#"{"name":"A"}"#)]);
        rejects(&[(MANUAL_GROUPS_KEY, r#"[{"name":"  ","neighborhoods":[],"districts":[]}]"#)]);
        let long = "a".repeat(81);
        rejects(&[(MANUAL_GROUPS_KEY, &format!(r#"[{{"name":"{long}","neighborhoods":[],"districts":[]}}]"#))]);
        rejects(&[(MANUAL_GROUPS_KEY, r#"[{"name":"A","neighborhoods":[1],"districts":[]}]"#)]);
        rejects(&[(MANUAL_GROUPS_KEY, r#"[{"name":"A","neighborhoods":"x","districts":[]}]"#)]);
    }

    #[test]
    fn validate_rejects_duplicate_names_case_insensitively_in_turkish() {
        rejects(&[(
            MANUAL_GROUPS_KEY,
            r#"[{"name":"ışık","neighborhoods":[],"districts":[]},{"name":"IŞIK","neighborhoods":[],"districts":[]}]"#,
        )]);
    }

    #[test]
    fn validate_accepts_name_of_exactly_eighty_characters() {
        let name = "a".repeat(80);
        let json = format!(r#"[{{"name":"{name}","neighborhoods":[],"districts":[]}}]"#);
        assert!(validate_settings(&entries(&[(MANUAL_GROUPS_KEY, &json)])).is_ok());
    }
}
