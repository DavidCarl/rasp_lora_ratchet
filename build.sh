PASSWORD=""

OPTION=$1

SERVER_IP=192.168.43.192
CLIENT_IP=192.168.43.206

function server(){
    cd server
    ~/.cargo/bin/cross build --target arm-unknown-linux-gnueabihf
    sshpass -p $PASSWORD scp target/arm-unknown-linux-gnueabihf/debug/rasp_lora_server pi@$SERVER_IP:/home/pi/
    cd ..
}

function client(){
    cd client
    ~/.cargo/bin/cross build --target arm-unknown-linux-gnueabihf
    sshpass -p $PASSWORD scp target/arm-unknown-linux-gnueabihf/debug/rasp_lora_client pi@$CLIENT_IP:/home/pi/
    cd ..
}

if [[ "$OPTION" == "both" || "$OPTION" == "" ]]; then
    echo "Building and uploading both"
    client
    server

elif [[ "$OPTION" == "client" ]]; then
    echo "Building and uploading client"
    client

elif [[ "$OPTION" == "server" ]]; then
    echo "Building and uploading server"
    server

else
    echo "Misisng options"
    echo "use: 'both' for client and server"
    echo "use: 'server' for server"
    echo "use: 'client' for client"
fi
