export const adminSections = [
  {
    to: '/admin/roles',
    label: '역할 관리',
    description: '관리자, 일반 사용자 등 계정에 부여할 수 있는 역할을 확인합니다.',
  },
  {
    to: '/admin/permissions',
    label: '권한 관리',
    description: '권한 코드와 각 권한이 허용하는 작업을 확인합니다.',
  },
] as const
