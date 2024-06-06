// Import necessary modules and libraries
mod utils;
use std::str::FromStr;

use shared_crypto::intent::Intent;
use sui_config::{sui_config_dir, SUI_KEYSTORE_FILENAME};
use sui_json_rpc_types::{SuiObjectDataOptions, SuiObjectResponse};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    types::{
        base_types::{ObjectID, ObjectRef, SequenceNumber}, 
        digests::{self, Digest, ObjectDigest}, 
        object, 
        programmable_transaction_builder::ProgrammableTransactionBuilder, 
        quorum_driver_types::ExecuteTransactionRequestType, 
        sui_serde::SuiStructTag, 
        transaction::{Argument, CallArg, Command, ObjectArg, Transaction, TransactionData}, 
        Identifier, 
        TypeTag
    }, 
    SuiClient, 
    SuiClientBuilder,
};
use utils::setup_for_write;

// This example demonstrates how to use programmable transactions to chain multiple
// actions into one transaction. The steps are as follows:
// 1) Retrieve two addresses from the local wallet.
// 2) Find a coin from the active address that contains Sui.
// 3) Split the coin into one coin of 1000 MIST and the remaining balance.
// 4) Transfer the split coin to the second Sui address.
// 5) Sign the transaction.
// 6) Execute the transaction.
// The program prints output for some of these actions.
// Finally, it prints the number of coins for the recipient address at the end.
// Running this program multiple times should show an increasing number of coins for the recipient address.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1) Get the Sui client, the sender, and recipient addresses for the transaction
    // and find the coin we will use as gas
    let (sui, sender, recipient) = setup_for_write().await?;

    // 2) Retrieve the coins for the sender address
    let coins = sui
        .coin_read_api()
        .get_coins(sender, None, None, None)
        .await?;
    // Use the first coin from the list as the gas coin
    let coin = coins.data.into_iter().next().unwrap();

    // 3) Create a new programmable transaction builder
    let mut ptb = ProgrammableTransactionBuilder::new();

    // 4) Fetch game room information
    // Define the game room object ID
    let game_room_id = ObjectID::from_hex_literal("0x52509952e7b80b08880238e9737e8f70e223418816e5a85bf82575ef84ecc545")
        .unwrap();
    // Define the object ID
    let object_id = ObjectID::from_hex_literal("0xb28e2aa6a21db55873a1b81983cbd19544459971b67ba2ddbd7b8d6575d7c2d1").unwrap();
    // Fetch the game room object details with specified options
    let object = sui.read_api().get_object_with_options(object_id,
            SuiObjectDataOptions {
                show_type: true,
                show_owner: true,
                show_previous_transaction: true,
                show_display: true,
                show_content: true,
                show_bcs: true,
                show_storage_rebate: true,
            },
        ).await?;

    // Get the version of the game room object
    let object_version = object.clone().data.unwrap().version;
    // Specify if the object is mutable
    let is_mutable = true;
    // Create a CallArg for the game room object
    let game_room_input = CallArg::Object(ObjectArg::SharedObject{
        id: game_room_id,
        initial_shared_version: object_version,
        mutable: is_mutable, 
    });
    // Add the game room object as an input to the transaction
    ptb.input(game_room_input);
    
    // 5) Fetch game card information
    // Define the game card object ID
    let game_card_id = ObjectID::from_hex_literal("0x440b328ba3c90f203f439f6fc4c5aa40b7ca41d28317d5bb9b6c0207cfebc693")
        .unwrap();
    // Fetch the game card object details with specified options
    let game_card_object = sui.read_api().get_object_with_options(game_card_id,
            SuiObjectDataOptions {
                show_type: true,
                show_owner: true,
                show_previous_transaction: true,
                show_display: true,
                show_content: true,
                show_bcs: true,
                show_storage_rebate: true,
            },
        ).await?;

    // Get the version of the game card object
    let game_card_version = game_card_object.clone().data.unwrap().version;
    // Get the digest of the game card object
    let game_card_digests = game_card_object.data.unwrap().digest;
    // Create an ObjectRef for the game card object
    let game_card_object_ref: ObjectRef = (game_card_id, game_card_version, game_card_digests);
    // Create a CallArg for the game card object
    let game_card_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(game_card_object_ref));
    // Add the game card object as an input to the transaction
    ptb.input(game_card_input);

    // 6) Add commands to the programmable transaction builder
    // Add a command to create a Move vector with one element
    ptb.command(Command::MakeMoveVec(None, vec![
        Argument::Input(1),
    ]));

    // Add a command to call the `create_room` function in the `gamecards` module
    ptb.command(Command::MoveCall(Box::new(
        sui_sdk::types::transaction::ProgrammableMoveCall {
            package: ObjectID::from_hex_literal("0xc74620c25579b75ac8f6d0d670a4663944ff7f29d6e856f6b33e0a35a34c5a06").unwrap(),
            module: Identifier::new("gamecards").unwrap(),
            function: Identifier::new("create_room").unwrap(),
            type_arguments: vec![],
            arguments: vec![
                Argument::Input(0),
                Argument::Result(0),
            ],
        }
    )));

    // 7) Finish building the transaction block by calling finish on the programmable transaction builder
    let builder = ptb.finish();

    // Define the gas budget for the transaction
    let gas_budget = 10_000_000;
    // Get the current reference gas price
    let gas_price = sui.read_api().get_reference_gas_price().await?;
    // Create the transaction data that will be sent to the network
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![coin.object_ref()],
        builder,
        gas_budget,
        gas_price,
    );

    // 8) Sign the transaction
    // Load the keystore from the Sui config directory
    let keystore = FileBasedKeystore::new(&sui_config_dir()?.join(SUI_KEYSTORE_FILENAME))?;
    // Sign the transaction data using the sender's key
    let signature = keystore.sign_secure(&sender, &tx_data, Intent::sui_transaction())?;

    // 9) Execute the transaction
    print!("Executing the transaction...");
    // Execute the transaction block and wait for local execution
    let transaction_response = sui
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    // Print the transaction response
    print!("done\nTransaction information: ");
    println!("{:?}", transaction_response);
    Ok(())
}
