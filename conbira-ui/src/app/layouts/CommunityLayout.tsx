import { SidebarShell } from './SidebarShell'

const communityItems = [
  { to: '/community/notice', label: '공지' },
  { to: '/community/free', label: '자유게시판' },
  { to: '/community/data', label: '자료실' },
] as const

export function CommunityLayout() {
  return <SidebarShell label="커뮤니티 메뉴" items={communityItems} />
}
