# Antes de lanzar el stack en el VPS, debes crearlas manualmente (si no existen) con el driver overlay (necesario para Swarm):


docker swarm init

docker network create --driver overlay stack-network

docker network create --driver overlay proxy-network

docker stack deploy -c docker-compose.vps.yml n8n-vps