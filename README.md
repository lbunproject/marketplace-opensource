# Backbone Labs - Necropolis (NFT Marketplace)

We have modified this smart contract to fix some issues we found in the source code.  Mostly fixing warnings but some small logic changes related to the royalty_fee setting. 

The deployed code to mainnet is `Code ID 1399` with hash `9c9ab88769c974e89a846977b499c5afe065c44b8e4bf67006c9e323d978bbe4`

We instantiated that code and the contract address for Necropolis is `terra1ej4cv98e9g2zjefr5auf2nwtq4xl3dm7x0qml58yna2ml2hk595s7gccs9`

## Knowhere Marketplace

Knowhere Marketplace open-source Terra-based NFT marketplace platform. 
ready to be deploy on any Cosmos-based chain with minimum setup effort.
.
Structure
- Contract (Cosmwasm Smart Contract Repository)
- Interface (React Frontend)

#### How to Deploy (Mac OS)

- Go into contract folder
- Run ```cargo build``` to compile the smart contract code.
- Run ```./build_release.sh ``` (Docker need to be install first).
- Run script in ```scripts/deploy_auction_testnet.ts ``` or ```scripts/deploy_auction_mainnet.ts ``` to deploy the smart contract.
- Go into interface folder.
- Run yarn to install necessary depedencies.
- Go into address.ts file to fill in marketplace address and nft address to point the correct smart contract into the interface.
- Now the frontend is ready to be deployed.







