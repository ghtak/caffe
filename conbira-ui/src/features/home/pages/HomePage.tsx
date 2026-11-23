import { Card } from '@/shared/components/ui/Card'

export function HomePage() {
  return (
    <main className="grid min-h-90 content-start gap-4 p-4 sm:p-6">
      <h1 className="text-xl font-semibold">홈</h1>
      <Card className="grid min-h-54 place-items-center border-dashed bg-muted/40 text-sm text-muted-foreground shadow-none">
        서비스 콘텐츠 영역
      </Card>
    </main>
  )
}
