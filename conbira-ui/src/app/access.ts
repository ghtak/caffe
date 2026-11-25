import { adminSections } from '@/features/admin'

export function can(permissions: readonly string[], required: readonly string[]) {
  return required.length > 0 && required.every((permission) => permissions.includes(permission))
}

export function visibleAdminSections(permissions: readonly string[]) {
  return adminSections.filter((section) => can(permissions, section.permissions))
}
