import { useRef } from 'react'
import { Button } from '@/shared/components/ui/Button'

export function Attachments({
  files,
  onChange,
  disabled,
}: {
  files: File[]
  onChange: (files: File[]) => void
  disabled: boolean
}) {
  const input = useRef<HTMLInputElement>(null)
  return (
    <section aria-label="첨부파일" className="grid gap-2">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-medium">첨부파일</h2>
        <Button
          type="button"
          variant="outline"
          disabled={disabled}
          onClick={() => input.current?.click()}
        >
          파일 추가
        </Button>
        <input
          ref={input}
          type="file"
          multiple
          className="sr-only"
          aria-label="파일 선택"
          disabled={disabled}
          onChange={(event) => {
            onChange([...files, ...Array.from(event.target.files ?? [])])
            event.target.value = ''
          }}
        />
      </div>
      {files.length > 0 && (
        <>
          <ul className="divide-y rounded-md border">
            {files.map((file, index) => (
              <li key={index} className="flex items-center justify-between gap-2 px-3 py-2 text-sm">
                <span className="min-w-0 break-all">{file.name}</span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={disabled}
                  aria-label={`${file.name} 삭제`}
                  onClick={() => onChange(files.filter((_, i) => i !== index))}
                >
                  삭제
                </Button>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  )
}
