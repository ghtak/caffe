import { z } from 'zod'

export const credentialsSchema = z.object({
  email: z.string().trim().pipe(z.email('이메일 주소를 입력해 주세요.')),
  password: z.string().min(1, '비밀번호를 입력해 주세요.'),
})

export type Credentials = z.infer<typeof credentialsSchema>

export const loginResponse = z.object({
  data: z.object({ access_token: z.string().min(1) }),
})

export const sessionClaims = z.object({
  exp: z.number().int().positive(),
  user: z.object({
    id: z.number().int().positive().optional(),
    permissions: z.array(z.string()).default([]),
  }),
})

export const sessionResponse = z.object({ data: sessionClaims })
export const refreshResponse = z.object({
  data: loginResponse.shape.data.extend({
    expires_at: sessionClaims.shape.exp,
    claims: sessionClaims.shape.user,
  }),
})

export const googleLoginStartResponse = z.object({
  data: z.object({
    authorization_url: z.url(),
    state: z.string().min(1),
    browser_proof: z.string().min(1),
  }),
})
