CREATE TABLE IF NOT EXISTS conbira_file_assets (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL, -- 'pending', 'attached', 'deleted'
    uploaded_by BIGINT NOT NULL REFERENCES conbira_auth_users (id) ON DELETE RESTRICT,
    s3_bucket TEXT NOT NULL,
    s3_object_key TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    content_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMPTZ
);

-- 배치 작업(만료된 pending 및 deleted 자산 정리)용 인덱스
CREATE INDEX IF NOT EXISTS conbira_file_assets_status_created_idx
    ON conbira_file_assets (status, created_at);

COMMENT ON TABLE conbira_file_assets IS 'S3에 저장된 순수 파일 자산의 메타데이터와 생명주기 상태';
COMMENT ON COLUMN conbira_file_assets.id IS '서버에서 발급한 파일 자산 ID';
COMMENT ON COLUMN conbira_file_assets.status IS '상태값 (pending: 임시업로드, attached: 도메인연결됨, deleted: 삭제대기)';
COMMENT ON COLUMN conbira_file_assets.uploaded_by IS '파일을 업로드한 내부 사용자 ID';
COMMENT ON COLUMN conbira_file_assets.s3_bucket IS '파일 객체를 저장한 S3 버킷';
COMMENT ON COLUMN conbira_file_assets.s3_object_key IS 'S3 버킷 내 고유 객체 키 (UUID 기반)';
COMMENT ON COLUMN conbira_file_assets.original_filename IS '업로드 시 전달된 원본 파일명';
COMMENT ON COLUMN conbira_file_assets.size_bytes IS '파일 크기 (바이트)';
COMMENT ON COLUMN conbira_file_assets.content_type IS '파일의 MIME 유형';
COMMENT ON COLUMN conbira_file_assets.created_at IS '메타데이터 등록 시각 (pending 만료 판단 기준)';
COMMENT ON COLUMN conbira_file_assets.updated_at IS '메타데이터 마지막 변경 시각';
COMMENT ON COLUMN conbira_file_assets.deleted_at IS '논리 삭제 시각 (S3 물리 삭제 배치 대상)';

INSERT INTO conbira_rbac_permissions (code, name, description)
VALUES
    ('file_asset.list.read', '파일 자산 목록 조회', '전체 스토리지 파일 자산 목록 및 상태를 조회'),
    ('file_asset.manage', '파일 자산 관리', '고아 파일 정리 및 파일 자산 강제 삭제 관리')
ON CONFLICT (code) DO NOTHING;

INSERT INTO conbira_rbac_role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM conbira_rbac_roles r
JOIN conbira_rbac_permissions p ON p.code IN ('file_asset.list.read', 'file_asset.manage')
WHERE r.code = 'admin'
ON CONFLICT (role_id, permission_id) DO NOTHING;
