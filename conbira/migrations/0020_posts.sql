-- ============================================================================
-- 1. 게시글 메타데이터 (목록 조회 및 상태 관리용 경량 테이블)
-- ============================================================================
CREATE TABLE IF NOT EXISTS conbira_posts (
    id                  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    board_slug          VARCHAR(64)  NOT NULL,
    category            VARCHAR(64)  NULL,
    author_id           BIGINT       NOT NULL REFERENCES conbira_auth_users (id) ON DELETE RESTRICT,
    title               VARCHAR(255) NOT NULL,
    excerpt             VARCHAR(500) NULL,
    attachment_group_id UUID         NULL,
    thumbnail_file_id   UUID         NULL,
    is_notice           BOOLEAN      NOT NULL DEFAULT FALSE,
    is_secret           BOOLEAN      NOT NULL DEFAULT FALSE,
    qna_status          VARCHAR(16)  NULL,
    accepted_comment_id BIGINT       NULL,
    view_count          BIGINT       NOT NULL DEFAULT 0,
    like_count          INT          NOT NULL DEFAULT 0,
    comment_count       INT          NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at          TIMESTAMPTZ  NULL,

    CONSTRAINT chk_conbira_posts_qna_status
        CHECK (qna_status IS NULL OR qna_status IN ('OPEN', 'RESOLVED'))
);

COMMENT ON TABLE conbira_posts IS '게시글 메타데이터 (목록 조회 및 상태 관리용 경량 테이블)';
COMMENT ON COLUMN conbira_posts.id IS '게시글 고유 ID';
COMMENT ON COLUMN conbira_posts.board_slug IS '게시판 구분 슬러그 (예: notice, free, gallery, blog, qna)';
COMMENT ON COLUMN conbira_posts.category IS '게시글 말머리/카테고리명';
COMMENT ON COLUMN conbira_posts.author_id IS '작성자 내부 사용자 ID (conbira_auth_users.id 참조)';
COMMENT ON COLUMN conbira_posts.title IS '게시글 제목';
COMMENT ON COLUMN conbira_posts.excerpt IS 'BLOG 레이아웃 목록 카드에 표시할 본문 요약문 (최대 500자)';
COMMENT ON COLUMN conbira_posts.attachment_group_id IS '첨부파일 그룹 ID (conbira_files.group_id 참조, 첨부파일 없으면 NULL)';
COMMENT ON COLUMN conbira_posts.thumbnail_file_id IS 'GALLERY/BLOG 레이아웃 목록 카드용 대표 썸네일 파일 ID (conbira_files.file_id 참조)';
COMMENT ON COLUMN conbira_posts.is_notice IS '상단 고정 공지 여부';
COMMENT ON COLUMN conbira_posts.is_secret IS '비밀글 여부 (작성자 및 관리자만 열람 가능)';
COMMENT ON COLUMN conbira_posts.qna_status IS 'QNA 레이아웃 전용 진행 상태 (OPEN: 답변대기, RESOLVED: 답변완료)';
COMMENT ON COLUMN conbira_posts.accepted_comment_id IS 'QNA 레이아웃 전용 채택된 답변 댓글 ID (conbira_post_comments.id 참조)';
COMMENT ON COLUMN conbira_posts.view_count IS '누적 조회수';
COMMENT ON COLUMN conbira_posts.like_count IS '누적 좋아요 수';
COMMENT ON COLUMN conbira_posts.comment_count IS '활성 댓글 수 (반정규화 카운터)';
COMMENT ON COLUMN conbira_posts.created_at IS '최초 작성 시각';
COMMENT ON COLUMN conbira_posts.updated_at IS '최종 수정 시각';
COMMENT ON COLUMN conbira_posts.deleted_at IS '논리 삭제 시각 (NULL이면 정상 활성 상태)';


-- ============================================================================
-- 2. 게시글 본문 (수직 분할: 상세 조회 시에만 PK 조인)
-- ============================================================================
CREATE TABLE IF NOT EXISTS conbira_post_contents (
    post_id        BIGINT      PRIMARY KEY REFERENCES conbira_posts (id) ON DELETE CASCADE,
    content        TEXT        NOT NULL,
    content_format VARCHAR(16) NOT NULL DEFAULT 'MARKDOWN',
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT chk_conbira_post_contents_format
        CHECK (content_format IN ('MARKDOWN', 'HTML'))
);

COMMENT ON TABLE conbira_post_contents IS '게시글 본문 데이터 (목록 I/O 부하 분리를 위한 1:1 수직 분할 테이블)';
COMMENT ON COLUMN conbira_post_contents.post_id IS '게시글 ID (conbira_posts.id FK 겸 PK)';
COMMENT ON COLUMN conbira_post_contents.content IS '게시글 본문 텍스트';
COMMENT ON COLUMN conbira_post_contents.content_format IS '본문 저장 포맷 (MARKDOWN | HTML)';
COMMENT ON COLUMN conbira_post_contents.updated_at IS '본문 최종 수정 시각';


-- ============================================================================
-- 3. 댓글 및 QnA 답변 (1-Depth 답글 + @멘션 구조)
-- ============================================================================
CREATE TABLE IF NOT EXISTS conbira_post_comments (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    post_id    BIGINT      NOT NULL REFERENCES conbira_posts (id) ON DELETE CASCADE,
    parent_id  BIGINT      NULL REFERENCES conbira_post_comments (id) ON DELETE CASCADE,
    author_id  BIGINT      NOT NULL REFERENCES conbira_auth_users (id) ON DELETE RESTRICT,
    content    TEXT        NOT NULL,
    is_secret  BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMPTZ NULL
);

COMMENT ON TABLE conbira_post_comments IS '게시글 댓글, 1-depth 답글 및 QnA 답변 테이블';
COMMENT ON COLUMN conbira_post_comments.id IS '댓글 고유 ID';
COMMENT ON COLUMN conbira_post_comments.post_id IS '소속 게시글 ID (conbira_posts.id FK)';
COMMENT ON COLUMN conbira_post_comments.parent_id IS '상위 루트 댓글 ID (최상위 댓글 또는 QnA 답변인 경우 NULL, 답글은 항상 루트 댓글 ID를 참조)';
COMMENT ON COLUMN conbira_post_comments.author_id IS '댓글 작성자 내부 사용자 ID (conbira_auth_users.id 참조)';
COMMENT ON COLUMN conbira_post_comments.content IS '댓글 본문 내용 (답글 대상은 @멘션으로 표기)';
COMMENT ON COLUMN conbira_post_comments.is_secret IS '비밀 댓글 여부';
COMMENT ON COLUMN conbira_post_comments.created_at IS '댓글 작성 시각';
COMMENT ON COLUMN conbira_post_comments.updated_at IS '댓글 수정 시각';
COMMENT ON COLUMN conbira_post_comments.deleted_at IS '논리 삭제 시각 (답글이 달린 루트 댓글 마스킹 보호용)';


-- ============================================================================
-- 4. 필수 인덱스 (Partial Index 적용)
-- ============================================================================
CREATE INDEX IF NOT EXISTS conbira_posts_board_list_idx
    ON conbira_posts (board_slug, is_notice DESC, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS conbira_posts_board_category_list_idx
    ON conbira_posts (board_slug, category, is_notice DESC, created_at DESC, id DESC)
    WHERE deleted_at IS NULL AND category IS NOT NULL;

CREATE INDEX IF NOT EXISTS conbira_posts_author_idx
    ON conbira_posts (author_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS conbira_post_comments_thread_idx
    ON conbira_post_comments (post_id, COALESCE(parent_id, id) ASC, parent_id NULLS FIRST, created_at ASC, id ASC);