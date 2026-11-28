import { POST_TITLE_MAX_LENGTH } from '../constants'
import { z } from 'zod'

export const postFormSchema = z.object({
  title: z
    .string()
    .trim()
    .min(1, '제목을 입력해 주세요.')
    .refine(
      (value) => [...value].length <= POST_TITLE_MAX_LENGTH,
      `제목은 ${POST_TITLE_MAX_LENGTH}자 이하로 입력해 주세요.`,
    ),
  content: z.string().refine((value) => value.trim().length > 0, '본문을 입력해 주세요.'),
})

export type PostFormValues = z.infer<typeof postFormSchema>
