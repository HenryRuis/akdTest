use aes::Aes128;
use akd::storage::StorageManager;
use akd::storage::memory::AsyncInMemoryDatabase;
use akd::ecvrf::HardCodedAkdVRF;
use akd::directory::Directory;
use akd::EpochHash;
use akd::{AkdLabel, AkdValue};
use akd::Digest;
use akd::HistoryParams;
use std::time::{Instant, Duration};
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use rand::Rng;
use rand_core::{OsRng, RngCore};
use std::collections::{HashMap, HashSet};
use std::mem;
use std::ops::Sub;
use mysql::{Pool, Row, params::Params, prelude::Queryable};
type Config = akd::WhatsAppV1Configuration;

#[tokio::main]
async fn main() {
    //test().await;
    let db = AsyncInMemoryDatabase::new();
    let storage_manager = StorageManager::new_no_cache(db);
    let vrf = HardCodedAkdVRF{};
    let mut akdmap: HashMap<i32, Directory<Config, AsyncInMemoryDatabase, HardCodedAkdVRF>> = HashMap::new();
    let mut keyword_existence: HashMap<i32, Vec<String>> = HashMap::new();
    //test_index_process(&mut akdmap).await;
    test_index_construction(&mut akdmap, &mut keyword_existence).await;
    /*
    for (patient_id, keywords) in &keyword_existence {
        println!("Patient ID: {}", patient_id);
        println!("Keywords:");
        // 遍历每个关键词列表
        for keyword in keywords {
            println!("{}|", keyword);
        }
    }
    */
    test_index_verify_time_space_all(&mut akdmap, &mut keyword_existence).await;
}

async fn test_index_process(akdmap: &mut HashMap<i32, Directory<Config, AsyncInMemoryDatabase, HardCodedAkdVRF>>) {
    let key0 =  b"ThisIs16ByteKey0";
    let key1 = b"ThisIs16ByteKey1";
    let kw = "apple";

    let time0 = Instant::now();

    let patient_id = 0;
    if akdmap.contains_key(&patient_id) == false {
        let db = AsyncInMemoryDatabase::new();
        let storage_manager = StorageManager::new_no_cache(db);
        let vrf = HardCodedAkdVRF {};
        let mut akd = Directory::<Config, _, _>::new(storage_manager, vrf)
            .await
            .expect("Could not create a new directory");
        akdmap.insert(patient_id, akd);
    }
    let time1 = Instant::now();

    let value = generate_random_string();
    let time2 = Instant::now();

    let akd = akdmap.get(&patient_id);
    let entries = vec![
        (AkdLabel::from(kw), AkdValue::from(&value)),
    ];

    let EpochHash(epoch, root_hash) = akd.expect("REASON").publish(entries)
        .await.expect("Error with publishing");
    let time3 = Instant::now();
    println!("Published epoch {} with root hash: {}", epoch, hex::encode(root_hash));


    let init_time = time1.duration_since(time0);
    let key_generate_time = time2.duration_since(time1);
    let index_update_time = time3.duration_since(time2);

    println!("Index init {:?}, key generate {:?}, and update: {:?}",
             init_time, key_generate_time, index_update_time);
}

#[derive(Debug, PartialEq, Eq)]
struct Record {
    obs_id: i32,
    patient_id: i32,
    concept_name: String,
    description: String,
}


async fn test_index_construction(akdmap: &mut HashMap<i32, Directory<Config, AsyncInMemoryDatabase, HardCodedAkdVRF>>, keyword_existence: &mut HashMap<i32, Vec<String>>) {
    println!("Index Construction Testing...");
    // 替换为您的数据库连接信息
    let db_url = "mysql://root:root1234@localhost:3306/foo";

    // 连接到数据库池
    let pool = Pool::new(db_url).expect("Failed to create pool");

    // 获取数据库连接
    let mut conn = pool.get_conn().expect("Failed to get connection");

    // 执行查询
    let result: Vec<Record> = conn.query_map(
        "SELECT obs_id, patient_id, concept_name, description FROM Vedrfolnir LIMIT 1000",
        |(obs_id, patient_id, concept_name, description)| {
            Record { obs_id, patient_id, concept_name, description }
        },
    ).expect("Failed to execute query");

    let mut total_time = Duration::from_secs(0);
    let mut i = 0;
    // 处理查询结果
    for record in result {
        //println!("{:?}", record);
        let patient_id = record.patient_id;
        if akdmap.contains_key(&patient_id) == false {
            //println!("new patient: {:?}", record.patient_id);
            let db = AsyncInMemoryDatabase::new();
            let storage_manager = StorageManager::new_no_cache(db);
            let vrf = HardCodedAkdVRF {};
            let akd = Directory::<Config, _, _>::new(storage_manager, vrf)
                .await
                .expect("Could not create a new directory");
            akdmap.insert(patient_id, akd);
            keyword_existence.insert(patient_id, Vec::new());
        }

        let mut akd = akdmap.get(&patient_id);

        let keyword_str = format!("{} {}", record.concept_name, record.description);
        let mut keyword = keyword_str.split_whitespace();
        let mut entries = vec![];

        let mut kwmap =  HashMap::new();
        let entry = keyword_existence
            .entry(patient_id)
            .or_insert(Vec::new());
        while let Some(kw) = keyword.next() {
            //println!("{}", kw);
            if !kwmap.contains_key(kw) {
                //println!("{:?} is not exist in this sentence ", kw);
                let value = generate_random_string();
                entries.push((AkdLabel::from(kw), AkdValue::from(&value)));
                kwmap.insert(kw, 1);
                if !entry.contains(&kw.to_string()) {
                    entry.push(kw.to_string())
                }

            }
        }
        let time2_1 = Instant::now();
        let EpochHash(epoch, root_hash) = akd.expect("REASON").publish(entries)
            .await.expect("Error with publishing");
        let time2_2 = Instant::now();

        total_time += time2_2.duration_since(time2_1);

        i += 1;

        if i % 1000 == 0 {
            println!("Average time cost for each data in {} data: {:?}", i, total_time / i);
        }
    }


}

async fn test_index_verify_time_space_all (akdmap: &mut HashMap<i32, Directory<Config, AsyncInMemoryDatabase, HardCodedAkdVRF>>, keyword_existence: &mut HashMap<i32, Vec<String>>) {
    println!("Index Verification Testing...");
    let mut find_proof = Duration::from_secs(0);
    let mut verify_proof = Duration::from_secs(0);
    let mut proof_size = 0;
    let mut key_size = 0;
    for (patient_id, akd) in akdmap {
        println!("Patient {:?} keyword checking...", patient_id);
        let mut one_patient_find_proof = Duration::from_secs(0);
        let mut one_patient_verify_proof = Duration::from_secs(0);
        let mut one_patient_proof_size = 0;
        let mut one_patient_key_size = 0;
        if let Some(keywords) = keyword_existence.get(&patient_id) {
            for kw in keywords.iter() {
                //println!("{}", kw);
                //println!("Patient {:?} keyword {:?} checking...", patient_id, kw);
                let time1 = Instant::now();
                let (history_proof, _) = akd.key_history(
                    &AkdLabel::from(kw),
                    HistoryParams::default(),
                ).await.expect("Could not generate proof");

                let (lookup_proof, epoch_hash) = akd.lookup(
                    AkdLabel::from(kw)
                ).await.expect("Could not generate proof");
                let time2 = Instant::now();


                one_patient_key_size += mem::size_of_val(&history_proof);

                let public_key = akd.get_public_key().await.expect("Could not fetch public key");

                let time3 = Instant::now();
                let key_history_result = akd::client::key_history_verify::<Config>(
                    public_key.as_bytes(),
                    epoch_hash.hash(),
                    epoch_hash.epoch(),
                    AkdLabel::from(kw),
                    history_proof,
                    akd::HistoryVerificationParams::default(),
                ).expect("Could not verify history");
                let time4 = Instant::now();

                one_patient_proof_size += mem::size_of_val(&key_history_result);
                let one_data_find_proof = time2.sub(time1);
                let one_data_verify_proof = time4.sub(time3);
                one_patient_find_proof += one_data_find_proof;
                one_patient_verify_proof += one_data_verify_proof;


            }
            find_proof += one_patient_find_proof;
            verify_proof += one_patient_verify_proof;
            key_size += one_patient_proof_size;
            proof_size += one_patient_proof_size;
        }
        println!("For patient {:?} Time cost: {:?} (find) + {:?} (verify) = {:?}. Storage cost: {:?} Byte (Proof) + {:?} Byte (Key) = {:?} Byte",
                 patient_id, one_patient_find_proof, one_patient_verify_proof, one_patient_verify_proof+one_patient_verify_proof
                    ,one_patient_proof_size, one_patient_key_size, one_patient_proof_size + one_patient_key_size);

    }
    println!(" Time cost: {:?} (find) + {:?} (verify) = {:?} Storage cost: {:?} Byte (Proof) + {:?} Byte (Key) = {:?} Byte",
             find_proof, verify_proof, verify_proof+verify_proof, proof_size, key_size, proof_size+key_size);

}

async fn test() {
    let time0 = Instant::now();
    type Config = akd::WhatsAppV1Configuration;

    let db = AsyncInMemoryDatabase::new();
    let storage_manager = StorageManager::new_no_cache(db);
    let vrf = HardCodedAkdVRF{};

    let mut akd = Directory::<Config, _, _>::new(storage_manager, vrf)
        .await
        .expect("Could not create a new directory");

    let time1 = Instant::now();

    let entries = vec![
        (AkdLabel::from("first entry"), AkdValue::from("first value")),
        (AkdLabel::from("second entry"), AkdValue::from("second value")),
        (AkdLabel::from("third entry"), AkdValue::from("second value")),
    ];

    let EpochHash(epoch, root_hash) = akd.publish(entries)
        .await.expect("Error with publishing");

    let time2 = Instant::now();
    //println!("Published epoch {} with root hash: {}", epoch, hex::encode(root_hash));

    let (lookup_proof, epoch_hash) = akd.lookup(
        AkdLabel::from("first entry")
    ).await.expect("Could not generate proof");

    let time3 = Instant::now();

    let public_key = akd.get_public_key().await.expect("Could not fetch public key");

    let time4 = Instant::now();

    let lookup_result = akd::client::lookup_verify::<Config>(
        public_key.as_bytes(),
        epoch_hash.hash(),
        epoch_hash.epoch(),
        AkdLabel::from("first entry"),
        lookup_proof,
    ).expect("Could not verify lookup proof");

    assert_eq!(
    lookup_result,
    akd::VerifyResult {
        epoch: 1,
        version: 1,
        value: AkdValue::from("first value"),
        },
    );
    let time5 = Instant::now();

    let EpochHash(epoch2, root_hash2) = akd.publish(
        vec![(AkdLabel::from("first entry"), AkdValue::from("updated value"))],
    ).await.expect("Error with publishing");

    let time6 = Instant::now();

    let (history_proof, _) = akd.key_history(
        &AkdLabel::from("first entry"),
        HistoryParams::default(),
    ).await.expect("Could not generate proof");

    let time7 = Instant::now();

    /*let key_history_result = akd::client::key_history_verify::<Config>(
        public_key.as_bytes(),
        epoch_hash.hash(),
        epoch_hash.epoch(),
        AkdLabel::from("first entry"),
        history_proof,
        akd::HistoryVerificationParams::default(),
    ).expect("Could not verify history");

    assert_eq!(
    key_history_result,
    vec![
        akd::VerifyResult {
            epoch: 2,
            version: 2,
            value: AkdValue::from("updated value"),
        },
        akd::VerifyResult {
            epoch: 1,
            version: 1,
            value: AkdValue::from("first value"),
        },],
    );
    let time8 = Instant::now();*/

    let initIndexTime = time1.sub(time0);
    let publish3keyword = time2.sub(time1);
    let lookupProof = time3.sub(time2);
    let getPublicKey = time4.sub(time3);
    let lookupResult = time5.sub(time4);
    let updateKeyword = time6.sub(time5);
    let getHistory = time7.sub(time6);
    //let verifyHistory = time8.sub(time7);

    println!("Index init {:?}, publish 3 keyword {:?}, lookup proof: {:?}, \
    get public key: {:?}, lookup result: {:?}, update keyword: {:?}, get history: {:?}, \
    ", initIndexTime, publish3keyword, lookupProof, getPublicKey, lookupResult, updateKeyword, getHistory);
}


fn generate_random_string() -> String {
    let mut rng = rand::thread_rng();
    let random_bytes: Vec<u8> = (0..16).map(|_| rng.gen()).collect();
    let random_string: String = random_bytes
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect();

    random_string
}