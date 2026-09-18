import { call } from './client'
import type { Company, NewCompany } from '../types/models'

export const companiesApi = {
  list: (): Promise<Company[]> => call('list_companies'),
  get: (id: number): Promise<Company> => call('get_company', { id }),
  create: (input: NewCompany): Promise<Company> => call('create_company', { input }),
  update: (id: number, input: NewCompany): Promise<Company> =>
    call('update_company', { id, input }),
  remove: (id: number): Promise<void> => call('delete_company', { id }),
  setLocation: (id: number, latitude: number, longitude: number): Promise<Company> =>
    call('set_company_location', { id, latitude, longitude }),
}
