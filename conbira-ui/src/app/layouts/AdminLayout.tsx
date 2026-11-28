import { visibleAdminSections } from '../access'
import { useSession } from '../providers/SessionProvider'
import { SidebarShell } from './SidebarShell'

export function AdminLayout() {
  const { session } = useSession()
  const sections =
    session.status === 'authenticated' ? visibleAdminSections(session.permissions) : []

  return (
    <SidebarShell
      label="관리 메뉴"
      items={[{ to: '/admin', label: '관리 홈' } as const, ...sections]}
    />
  )
}
