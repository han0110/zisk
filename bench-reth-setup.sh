tar -xzf block.tar.gz

cargo build --release --features gpu

./target/release/cargo-zisk check-setup -a

./target/release/cargo-zisk rom-setup --elf ./stateless-validator-reth

# ./target/release/zisk-coordinator --webhook-url http://localhost:50052

# CUDA_VISIBLE_DEVICES=0,1,2,3 numactl --cpunodebind=0 --membind=0 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23115
# CUDA_VISIBLE_DEVICES=4,5,6,7 numactl --cpunodebind=1 --membind=1 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23118

# NO_COLOR=1 RUST_LOG=info ./target/release/bench-reth > bench-reth.log
