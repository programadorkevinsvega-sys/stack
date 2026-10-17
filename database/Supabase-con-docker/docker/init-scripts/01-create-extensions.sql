-- Script de inicialización para Supabase PostgreSQL
-- Autor: Supabase Docker Project
-- Fecha: $(date)

-- Crear extensiones necesarias para Supabase
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "pgjwt";
CREATE EXTENSION IF NOT EXISTS "pgsodium";
CREATE EXTENSION IF NOT EXISTS "pg_graphql";
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";
CREATE EXTENSION IF NOT EXISTS "btree_gin";
CREATE EXTENSION IF NOT EXISTS "btree_gist";

-- Crear esquemas necesarios
CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS storage;
CREATE SCHEMA IF NOT EXISTS realtime;
CREATE SCHEMA IF NOT EXISTS functions;

-- Configurar permisos básicos
GRANT USAGE ON SCHEMA auth TO postgres;
GRANT USAGE ON SCHEMA storage TO postgres;
GRANT USAGE ON SCHEMA realtime TO postgres;
GRANT USAGE ON SCHEMA functions TO postgres;

-- Crear usuario para replicación si no existe
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'replicator') THEN
        CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD 'replicator123';
    END IF;
END
$$;

-- Configurar replicación si está habilitada
DO $$
BEGIN
    IF current_setting('postgresql.replication_mode', true) = 'master' THEN
        -- Configurar replicación master
        ALTER SYSTEM SET wal_level = replica;
        ALTER SYSTEM SET max_wal_senders = 10;
        ALTER SYSTEM SET max_replication_slots = 10;
        ALTER SYSTEM SET hot_standby = on;
        ALTER SYSTEM SET archive_mode = on;
        ALTER SYSTEM SET archive_command = 'test ! -f /var/lib/postgresql/archive/%f && cp %p /var/lib/postgresql/archive/%f';
        
        -- Crear directorio de archivos si no existe
        PERFORM pg_create_physical_replication_slot('replica_slot');
    END IF;
END
$$;

-- Configurar clustering si está habilitado
DO $$
BEGIN
    IF current_setting('postgresql.cluster_mode', true) = 'cluster' THEN
        -- Configurar clustering
        ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements';
        ALTER SYSTEM SET track_activities = on;
        ALTER SYSTEM SET track_counts = on;
        ALTER SYSTEM SET track_io_timing = on;
        ALTER SYSTEM SET track_functions = all;
    END IF;
END
$$;

-- Crear tablas básicas para Supabase
CREATE TABLE IF NOT EXISTS auth.users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    encrypted_password TEXT,
    email_confirmed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS storage.buckets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner UUID REFERENCES auth.users(id),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    public BOOLEAN DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS storage.objects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bucket_id TEXT REFERENCES storage.buckets(id),
    name TEXT NOT NULL,
    owner UUID REFERENCES auth.users(id),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB DEFAULT '{}',
    path_tokens TEXT[] DEFAULT '{}'
);

-- Crear índices para optimización
CREATE INDEX IF NOT EXISTS idx_auth_users_email ON auth.users(email);
CREATE INDEX IF NOT EXISTS idx_storage_objects_bucket_id ON storage.objects(bucket_id);
CREATE INDEX IF NOT EXISTS idx_storage_objects_name ON storage.objects(name);
CREATE INDEX IF NOT EXISTS idx_storage_objects_owner ON storage.objects(owner);

-- Configurar Row Level Security
ALTER TABLE auth.users ENABLE ROW LEVEL SECURITY;
ALTER TABLE storage.buckets ENABLE ROW LEVEL SECURITY;
ALTER TABLE storage.objects ENABLE ROW LEVEL SECURITY;

-- Crear políticas básicas de seguridad
CREATE POLICY "Users can view own profile" ON auth.users
    FOR SELECT USING (auth.uid() = id);

CREATE POLICY "Users can update own profile" ON auth.users
    FOR UPDATE USING (auth.uid() = id);

CREATE POLICY "Users can view own buckets" ON storage.buckets
    FOR SELECT USING (auth.uid() = owner);

CREATE POLICY "Users can view own objects" ON storage.objects
    FOR SELECT USING (auth.uid() = owner);

-- Crear función para obtener UID del usuario actual
CREATE OR REPLACE FUNCTION auth.uid()
RETURNS UUID
LANGUAGE SQL STABLE
AS $$
    SELECT COALESCE(
        current_setting('request.jwt.claim.sub', true)::UUID,
        current_setting('request.jwt.claim.user_id', true)::UUID,
        (current_setting('request.jwt.claims', true)::JSONB ->> 'sub')::UUID
    );
$$;

-- Crear función para verificar si el usuario está autenticado
CREATE OR REPLACE FUNCTION auth.role()
RETURNS TEXT
LANGUAGE SQL STABLE
AS $$
    SELECT COALESCE(
        current_setting('request.jwt.claim.role', true),
        (current_setting('request.jwt.claims', true)::JSONB ->> 'role')
    );
$$;

-- Configurar logging
ALTER SYSTEM SET log_statement = 'all';
ALTER SYSTEM SET log_min_duration_statement = 1000;
ALTER SYSTEM SET log_line_prefix = '%t [%p]: [%l-1] user=%u,db=%d,app=%a,client=%h ';

-- Crear función para limpiar logs antiguos
CREATE OR REPLACE FUNCTION cleanup_old_logs()
RETURNS VOID
LANGUAGE plpgsql
AS $$
BEGIN
    -- Limpiar logs de más de 30 días
    DELETE FROM pg_stat_statements WHERE query_start < NOW() - INTERVAL '30 days';
    
    -- Limpiar objetos de storage huérfanos
    DELETE FROM storage.objects 
    WHERE created_at < NOW() - INTERVAL '30 days' 
    AND owner IS NULL;
END;
$$;

-- Crear job para limpieza automática (si está disponible)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        -- Programar limpieza diaria a las 2:00 AM
        PERFORM cron.schedule('cleanup-logs', '0 2 * * *', 'SELECT cleanup_old_logs();');
    END IF;
END
$$;

-- Mensaje de confirmación
DO $$
BEGIN
    RAISE NOTICE 'Supabase PostgreSQL inicializado correctamente';
    RAISE NOTICE 'Extensiones creadas: uuid-ossp, pgcrypto, pgjwt, pgsodium, pg_graphql';
    RAISE NOTICE 'Esquemas creados: auth, storage, realtime, functions';
    RAISE NOTICE 'Tablas creadas: auth.users, storage.buckets, storage.objects';
    RAISE NOTICE 'Políticas de seguridad configuradas';
    RAISE NOTICE 'Funciones de autenticación creadas';
END
$$;
