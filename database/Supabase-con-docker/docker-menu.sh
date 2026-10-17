#!/bin/bash

clear

# --- Función principal del menú ---
show_menu() {
    echo "========== 🐳 MENÚ DE COMANDOS DOCKER =========="
    echo "1) Mostrar versión de Docker"
    echo "2) Mostrar imágenes locales"
    echo "3) Listar contenedores activos"
    echo "4) Listar todos los contenedores"
    echo "5) Ejecutar 'hello-world'"
    echo "6) Iniciar contenedor interactivo (bash)"
    echo "7) Ver logs de un contenedor"
    echo "8) Ejecutar un comando dentro de un contenedor"
    echo "9) Eliminar contenedor detenido"
    echo "10) Eliminar imagen"
    echo "11) Crear volumen Docker"
    echo "12) Listar volúmenes"
    echo "13) Eliminar volumen"
    echo "14) Mostrar redes Docker"
    echo "15) Ejecutar docker compose up"
    echo "16) Ejecutar docker compose down"
    echo "17) Limpiar sistema (docker system prune)"
    echo "0) Salir"
    echo "==============================================="
    echo -n "Selecciona una opción: "
}

read_container_name() {
    read -p "🆔 Ingresa el nombre o ID del contenedor: " CONTAINER
}

read_image_name() {
    read -p "📦 Ingresa el nombre de la imagen: " IMAGE
}

read_volume_name() {
    read -p "📂 Ingresa el nombre del volumen: " VOLUME
}

while true; do
    show_menu
    read opt
    echo ""

    case $opt in
        1) docker version ;;
        2) docker images ;;
        3) docker ps ;;
        4) docker ps -a ;;
        5) docker run hello-world ;;
        6)
            read -p "📦 Nombre de la imagen (por ejemplo ubuntu): " IMG
            docker run -it $IMG bash
            ;;
        7)
            read_container_name
            docker logs "$CONTAINER"
            ;;
        8)
            read_container_name
            docker exec -it "$CONTAINER" bash
            ;;
        9)
            docker ps -a
            read_container_name
            docker rm "$CONTAINER"
            ;;
        10)
            docker images
            read_image_name
            docker rmi "$IMAGE"
            ;;
        11)
            read_volume_name
            docker volume create "$VOLUME"
            ;;
        12)
            docker volume ls
            ;;
        13)
            docker volume ls
            read_volume_name
            docker volume rm "$VOLUME"
            ;;
        14)
            docker network ls
            ;;
        15)
            docker compose up -d
            ;;
        16)
            docker compose down
            ;;
        17)
            docker system prune -f
            ;;
        0)
            echo "👋 Saliendo..."
            exit 0
            ;;
        *)
            echo "❌ Opción inválida. Intenta de nuevo."
            ;;
    esac

    echo ""
    read -p "Presiona ENTER para continuar..." dummy
    clear
done
