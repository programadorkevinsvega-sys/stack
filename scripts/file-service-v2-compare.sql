\set ON_ERROR_STOP on
BEGIN;
INSERT INTO vector_migration_runs(id, operation, source_schema, target_schema, status, actor, details)
VALUES (:'run_id', 'compare', 'embedding_chunks_v1', 'embedding_vectors_v2', 'running', :'actor', '{}'::jsonb);

INSERT INTO vector_migration_reports(
    id,run_id,user_id,source_rows,target_rows,source_users,target_users,
    cross_user_mismatches,hash_mismatches
)
SELECT md5(:'run_id' || ':' || c.user_id::text)::uuid, :'run_id', c.user_id,
       COUNT(*)::BIGINT,
       (SELECT COUNT(*) FROM embedding_chunk_refs r WHERE r.user_id=c.user_id),
       1,
       (SELECT COUNT(DISTINCT r.user_id) FROM embedding_chunk_refs r WHERE r.user_id=c.user_id),
       COUNT(*) FILTER (WHERE v.user_id<>c.user_id)::BIGINT,
       COUNT(*) FILTER (WHERE v.content_hash<>c.content_hash)::BIGINT
FROM embedding_chunks c
LEFT JOIN embedding_vectors v
  ON v.user_id=c.user_id AND v.content_hash=c.content_hash
 AND v.embedding_model=c.embedding_model AND v.embedding_version=c.embedding_version
GROUP BY c.user_id
ON CONFLICT (id) DO NOTHING;

UPDATE vector_migration_runs
SET status='completed', completed_at=now(),
    details=details || jsonb_build_object(
      'cross_user_mismatches', (SELECT COALESCE(SUM(cross_user_mismatches),0) FROM vector_migration_reports WHERE run_id=:'run_id'),
      'hash_mismatches', (SELECT COALESCE(SUM(hash_mismatches),0) FROM vector_migration_reports WHERE run_id=:'run_id'))
WHERE id=:'run_id';
COMMIT;
