// PIN girdisini yalnız rakamlarla ve en fazla 6 haneyle sınırlayan bir girdi
// maskesidir — DOĞRULAMA DEĞİL. 4-6 hane kuralı ve rakam zorunluluğu arka
// uçta `AppError::Validation` ile uygulanır; burada yalnızca yazması kolay
// olsun diye girişi kırpıyoruz.
const MAX_PIN_LENGTH = 6

/** PIN alanına girilen ham metni rakam-dışı karakterlerden arındırır ve kırpar. */
export function sanitizePinInput(raw: string | null | undefined): string {
  return (raw ?? '').replace(/\D/g, '').slice(0, MAX_PIN_LENGTH)
}
