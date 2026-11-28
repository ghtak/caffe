import { boards, type BoardSlug } from './constants'

export function findBoard(slug: string) {
  return Object.hasOwn(boards, slug) ? boards[slug as BoardSlug] : undefined
}
