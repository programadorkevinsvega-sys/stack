UPDATE embedding_vectors
SET collection_name = CASE
        WHEN collection_name = 'nexus_embeddings_v2'
            THEN collection_name
        WHEN collection_name LIKE 'nexus_embeddings_%_v2'
            THEN 'nexus_embeddings_v2_' ||
                substring(
                    collection_name
                    FROM length('nexus_embeddings_') + 1
                    FOR length(collection_name) -
                        length('nexus_embeddings_') - length('_v2')
                )
        ELSE collection_name
    END,
    qdrant_synced = false,
    updated_at = now()
WHERE collection_name = 'nexus_embeddings_v2'
   OR collection_name LIKE 'nexus_embeddings_%_v2';

UPDATE embedding_vector_deletions AS deletion
SET collection_name = vector.collection_name,
    updated_at = now()
FROM embedding_vectors AS vector
WHERE vector.id = deletion.embedding_vector_id
  AND deletion.collection_name <> vector.collection_name;
