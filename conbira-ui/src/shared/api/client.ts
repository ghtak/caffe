import ky from 'ky'

export const apiClient = ky.create({ baseUrl: globalThis.location.origin })
