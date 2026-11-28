-- DELETE FROM conbira_rbac_role_permissions
-- WHERE permission_id IN (
--     SELECT id FROM conbira_rbac_permissions
--     WHERE code IN ('file.list.read', 'file.manage')
-- );

-- DELETE FROM conbira_rbac_permissions
-- WHERE code IN ('file.list.read', 'file.manage');
