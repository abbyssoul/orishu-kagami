#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu::model::cluster::OperationId;
use orishu_membership::testing;
use orishu_worker::peer::{
    catchup::wire::{Reply, Request},
    codec,
};

fuzz_target!(|data: &[u8]| {
    let _ = codec::decode::<Request>(data);
    let _ = codec::decode_frame::<Request>(data);
    let formation = testing::formation();
    let source = "node-0000".parse().unwrap();
    let request: OperationId = "fuzz-request".parse().unwrap();
    if let Ok(reply) = Reply::decode(data, &formation, &source, &request) {
        assert_eq!(reply.formation_id, formation);
        assert_eq!(reply.sender_id, source);
        assert_eq!(reply.request_id, request);
    }
    // Bind arbitrary page bytes to a real, synthetic source descriptor. The
    // receiver must reject wrong identities/order/digests and incomplete state.
    let model = testing::model_with_members(5);
    let frozen = orishu_worker::peer::catchup::Frozen::capture(&model, 1).unwrap();
    let receiver =
        orishu_worker::peer::catchup::Receiver::new(frozen.descriptor().clone()).unwrap();
    if let Ok(receiver) = receiver.push(data) {
        if let Ok(baseline) = receiver.finish_baseline() {
            assert_eq!(&baseline.formation_id, model.formation());
            assert_eq!(baseline.snapshot, 1);
        }
    }
});
