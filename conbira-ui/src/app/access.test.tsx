import { describe, expect, it } from 'vitest'
import { can } from './access'

describe('contract', () => {
  it('빈 권한 조건은 접근을 허용하지 않는다', () => {
    // 필요한 권한을 지정하지 않으면 접근을 거부한다.
    expect(can(['auth.user.read'], [])).toBe(false)
  })
})
