-- Añadir columna is_anonymous si no existe (auth schema)
ALTER TABLE IF EXISTS auth.users ADD COLUMN IF NOT EXISTS is_anonymous boolean NOT NULL DEFAULT false;

-- Crear esquema supabase_migrations si no existe
CREATE SCHEMA IF NOT EXISTS supabase_migrations AUTHORIZATION postgres;

-- Crear tabla de migraciones si no existe
CREATE TABLE IF NOT EXISTS supabase_migrations.schema_migrations (
    version text NOT NULL,
    name text,
    inserted_at timestamptz DEFAULT now(),
    PRIMARY KEY (version)
);

-- Conceder permisos
GRANT ALL ON SCHEMA supabase_migrations TO postgres;
GRANT ALL ON ALL TABLES IN SCHEMA supabase_migrations TO postgres;

-- Storage schema para que las migraciones funcionen
CREATE SCHEMA IF NOT EXISTS storage AUTHORIZATION supabase_storage_admin;
GRANT ALL ON SCHEMA storage TO supabase_storage_admin;
GRANT ALL ON SCHEMA storage TO postgres;
GRANT USAGE ON SCHEMA storage TO supabase_admin;
GRANT ALL ON SCHEMA storage TO supabase_admin;
GRANT SELECT ON ALL TABLES IN SCHEMA storage TO supabase_admin;

-- Roles necesarios para storage
DO $do$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'service_role') THEN CREATE ROLE service_role NOINHERIT; END IF; END $do$;
DO $do$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'authenticated') THEN CREATE ROLE authenticated NOINHERIT; END IF; END $do$;
GRANT service_role TO authenticator;
ALTER ROLE anon INHERIT;
ALTER ROLE authenticator INHERIT;

-- Conceder roles para storage
GRANT service_role TO supabase_storage_admin;
GRANT USAGE ON SCHEMA storage TO service_role;
GRANT USAGE ON SCHEMA storage TO anon;
GRANT SELECT ON ALL TABLES IN SCHEMA storage TO service_role;
GRANT SELECT ON ALL TABLES IN SCHEMA storage TO anon;
GRANT ALL ON ALL TABLES IN SCHEMA storage TO service_role;
GRANT ALL ON ALL TABLES IN SCHEMA storage TO anon;

-- Parámetros custom para storage
ALTER DATABASE supabase SET request.jwt.claim.sub = '';
ALTER DATABASE supabase SET request.jwt.claim.role = '';
ALTER DATABASE supabase SET request.jwt = '';
ALTER DATABASE supabase SET request.jwt.claims = '';
ALTER DATABASE supabase SET request.headers = '';
ALTER DATABASE supabase SET request.method = '';
ALTER DATABASE supabase SET request.path = '';
