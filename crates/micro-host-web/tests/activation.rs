use micro_host_web::ActivationQueue;
use micro_ir::FunctionId;

#[test]
fn activations_are_shared_and_fifo() {
    let producer = ActivationQueue::default();
    let mut consumer = producer.clone();
    producer.push(FunctionId(4));
    producer.push(FunctionId(9));
    assert_eq!(consumer.pop(), Some(FunctionId(4)));
    assert_eq!(consumer.pop(), Some(FunctionId(9)));
    assert_eq!(consumer.pop(), None);
}
