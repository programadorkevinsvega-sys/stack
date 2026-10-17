\set ON_ERROR_STOP on
BEGIN;

INSERT INTO vector_migration_runs(id, operation, source_schema, target_schema, status, actor, details)
VALUES (:'run_id', 'backfill', 'embedding_chunks_v1', 'embedding_vectors_v2', 'running', :'actor', '{"mode":"idempotent","provider_calls":0}'::jsonb);

-- The backfill never calls OpenRouter. Existing vectors are copied by
-- user/hash/model/version and are marked unsynced so file-service maintenance
-- can safely publish them to nexus_embeddings_v2.
INSERT INTO embedding_vectors(
    id,user_id,content_hash,content,embedding_model,embedding_version,
    strategy_family,embedding,token_count,vector_dimension,vector_bytes,
    collection_name,qdrant_point_id,qdrant_synced,status,reference_count,
    created_at,updated_at
)
SELECT DISTINCT ON (c.user_id,c.content_hash,c.embedding_model,c.embedding_version)
    c.id,c.user_id,c.content_hash,c.content,c.embedding_model,
    c.embedding_version,'legacy-v1',c.embedding,c.token_count,
    jsonb_array_length(c.embedding),c.vector_bytes,'nexus_embeddings_v2',
    c.id,false,'ready',0,c.created_at,now()
FROM embedding_chunks c
WHERE jsonb_typeof(c.embedding)='array'
  AND jsonb_array_length(c.embedding)>0
ORDER BY c.user_id,c.content_hash,c.embedding_model,c.embedding_version,c.created_at,c.id
ON CONFLICT(user_id,content_hash,embedding_model,embedding_version) DO UPDATE
SET content=EXCLUDED.content,
    embedding=EXCLUDED.embedding,
    token_count=EXCLUDED.token_count,
    vector_dimension=EXCLUDED.vector_dimension,
    vector_bytes=EXCLUDED.vector_bytes,
    qdrant_synced=false,
    updated_at=now();

INSERT INTO embedding_chunk_refs(
    id,user_id,file_id,document_id,chunk_id,chunk_index,
    strategy_version,embedding_model,embedding_version,embedding_vector_id,created_at
)
SELECT c.id,c.user_id,c.file_id,c.document_id,c.chunk_id,c.chunk_index,
       c.strategy_version,c.embedding_model,c.embedding_version,v.id,c.created_at
FROM embedding_chunks c
JOIN embedding_vectors v
  ON v.user_id=c.user_id AND v.content_hash=c.content_hash
 AND v.embedding_model=c.embedding_model
 AND v.embedding_version=c.embedding_version
ON CONFLICT DO NOTHING;

UPDATE embedding_vectors v
SET reference_count=(SELECT COUNT(*) FROM embedding_chunk_refs r WHERE r.embedding_vector_id=v.id),
    updated_at=now()
WHERE v.status='ready';

INSERT INTO vector_migration_reports(
    id,run_id,user_id,source_rows,target_rows,source_users,target_users,
    cross_user_mismatches,hash_mismatches
)
SELECT md5(:'run_id' || ':' || c.user_id::text)::uuid, :'run_id', c.user_id,
       COUNT(*)::BIGINT,
       (SELECT COUNT(*) FROM embedding_chunk_refs r WHERE r.user_id=c.user_id),
       1,1,0,
       COUNT(*) FILTER (WHERE v.user_id<>c.user_id OR v.content_hash<>c.content_hash)::BIGINT
FROM embedding_chunks c
LEFT JOIN embedding_vectors v ON v.id=c.id
GROUP BY c.user_id
ON CONFLICT (id) DO NOTHING;

UPDATE vector_migration_runs
SET status='completed', completed_at=now(),
    details=details || jsonb_build_object('completed_at', now())
WHERE id=:'run_id';
COMMIT;
