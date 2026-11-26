import { z } from 'zod'

export const fileSummarySchema = z.object({
  file_id: z.uuid(),
  group_id: z.uuid(),
  status: z.enum(['pending', 'attached', 'deleted']),
  original_filename: z.string(),
  size_bytes: z.number().int().nonnegative(),
  content_type: z.string(),
  created_at: z.iso.datetime({ offset: true }),
})
export type FileSummary = z.infer<typeof fileSummarySchema>
export type FileListPageProps = { token: string; canManage: boolean }
export type AttachFilesRequest = {
  resource_type: string
  resource_key: string
  attach_file_ids: string[]
  detach_file_ids: string[]
}
