export const boards = {
  notice: { title: '공지', path: '/community/notice', writePermission: 'board.notice.write' },
  free: { title: '자유게시판', path: '/community/free', writePermission: 'board.free.write' },
  data: { title: '자료실', path: '/community/data', writePermission: 'board.data.write' },
} as const
export type BoardSlug = keyof typeof boards

export const POST_PAGE_SIZE = 20
export const POST_TITLE_MAX_LENGTH = 255
export const POST_CONTENT_ROWS = 14
export const COMMENT_CONTENT_ROWS = 3
