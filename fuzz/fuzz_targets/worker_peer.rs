#![no_main]
use libfuzzer_sys::fuzz_target;
use orishu_membership::{Message, SenderIdentity, testing, update};
use orishu_worker::peer::wire::{self, Transport};

fuzz_target!(|data: &[u8]| {
    let model = testing::model_with_members(5);
    let peer = "node-0000".parse().unwrap();
    let mut context = testing::peer_context(&model, &peer, 0);
    for applicant in [false, true] {
        if applicant {
            context.sender = SenderIdentity::Applicant("worker-applicant".parse().unwrap());
        }
        for transport in [Transport::Datagram, Transport::Stream] {
            if let Ok(decoded) = wire::profile5::decode(data, &context, transport) {
                assert_eq!(decoded.input.context.formation, context.formation);
                assert_eq!(decoded.input.context.sender, context.sender);
                let result = update(model.clone(), Message::Peer(decoded.input));
                assert_eq!(result.model.formation(), model.formation());
                assert!(result.model.members().len() <= result.model.limits().max_members());
                let mut wrong = context.clone();
                wrong.authenticated = false;
                assert!(wire::decode(data, &wrong, transport).is_err());
                wrong.authenticated = true;
                wrong.formation = "other-formation".parse().unwrap();
                assert!(wire::decode(data, &wrong, transport).is_err());
            }
        }
    }
});
