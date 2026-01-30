#!/bin/bash

source ~/.bashrc

# Function to cleanup background processes
cleanup() {
    echo "Cleaning up processes..."
    if [ ! -z "$COORDINATOR_PID" ]; then
        kill $COORDINATOR_PID 2>/dev/null
    fi
    if [ ! -z "$DMON_PID" ]; then
        kill $DMON_PID 2>/dev/null
    fi
    for pid in "${WORKER_PIDS[@]}"; do
        kill $pid 2>/dev/null
    done
    wait 2>/dev/null

    sleep 60

    WORKER_PIDS=()
    COORDINATOR_PID=""
    DMON_PID=""
}

# --- Bench 1 worker each 1 gpu ---
echo "Starting Bench 1 worker each 1 gpu..."
./target/release/zisk-coordinator &
COORDINATOR_PID=$!

CUDA_VISIBLE_DEVICES=0 numactl --cpunodebind=0 --membind=0 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23115 -v > ./bench-reth-logs/1-worker-each-1-gpu-worker-0.log 2>&1 &
WORKER_PIDS=($!)

nvidia-smi dmon -s pucvmet > ./bench-reth-logs/1-worker-each-1-gpu-dmon.log 2>&1 &
DMON_PID=$!

sleep 60
for block_num in {24172600..24172649}; do
    echo "Running block rpc_block_${block_num}..."
    ./target/release/bench-reth -c 10 -i ./block/rpc_block_${block_num}
done
cleanup

# --- Bench 1 worker each 2 gpu ---
echo "Starting Bench 1 worker each 2 gpu..."
./target/release/zisk-coordinator &
COORDINATOR_PID=$!

CUDA_VISIBLE_DEVICES=0,1 numactl --cpunodebind=0 --membind=0 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23115 -v > ./bench-reth-logs/1-worker-each-2-gpu-worker-0.log 2>&1 &
WORKER_PIDS=($!)

nvidia-smi dmon -s pucvmet > ./bench-reth-logs/1-worker-each-2-gpu-dmon.log 2>&1 &
DMON_PID=$!

sleep 60
for block_num in {24172600..24172649}; do
    echo "Running block rpc_block_${block_num}..."
    ./target/release/bench-reth -c 10 -i ./block/rpc_block_${block_num}
done
cleanup

# --- Bench 1 worker each 4 gpu ---
echo "Starting Bench 1 worker each 4 gpu..."
./target/release/zisk-coordinator &
COORDINATOR_PID=$!

CUDA_VISIBLE_DEVICES=0,1,2,3 numactl --cpunodebind=0 --membind=0 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23115 -v > ./bench-reth-logs/1-worker-each-4-gpu-worker-0.log 2>&1 &
WORKER_PIDS=($!)

nvidia-smi dmon -s pucvmet > ./bench-reth-logs/1-worker-each-4-gpu-dmon.log 2>&1 &
DMON_PID=$!

sleep 60
for block_num in {24172600..24172649}; do
    echo "Running block rpc_block_${block_num}..."
    ./target/release/bench-reth -c 10 -i ./block/rpc_block_${block_num}
done
cleanup

# --- Bench 2 worker each 1 gpu ---
echo "Starting Bench 2 worker each 1 gpu..."
./target/release/zisk-coordinator &
COORDINATOR_PID=$!

CUDA_VISIBLE_DEVICES=0 numactl --cpunodebind=0 --membind=0 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23115 -v > ./bench-reth-logs/2-worker-each-1-gpu-worker-0.log 2>&1 &
WORKER_PIDS=($!)

CUDA_VISIBLE_DEVICES=4 numactl --cpunodebind=1 --membind=1 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23118 -v > ./bench-reth-logs/2-worker-each-1-gpu-worker-1.log 2>&1 &
WORKER_PIDS+=($!)

nvidia-smi dmon -s pucvmet > ./bench-reth-logs/2-worker-each-1-gpu-dmon.log 2>&1 &
DMON_PID=$!

sleep 60
for block_num in {24172600..24172649}; do
    echo "Running block rpc_block_${block_num}..."
    ./target/release/bench-reth -c 20 -i ./block/rpc_block_${block_num}
done
cleanup

# --- Bench 2 worker each 2 gpu ---
echo "Starting Bench 2 worker each 2 gpu..."
./target/release/zisk-coordinator &
COORDINATOR_PID=$!

CUDA_VISIBLE_DEVICES=0,1 numactl --cpunodebind=0 --membind=0 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23115 -v > ./bench-reth-logs/2-worker-each-2-gpu-worker-0.log 2>&1 &
WORKER_PIDS=($!)

CUDA_VISIBLE_DEVICES=4,5 numactl --cpunodebind=1 --membind=1 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23118 -v > ./bench-reth-logs/2-worker-each-2-gpu-worker-1.log 2>&1 &
WORKER_PIDS+=($!)

nvidia-smi dmon -s pucvmet > ./bench-reth-logs/2-worker-each-2-gpu-dmon.log 2>&1 &
DMON_PID=$!

sleep 60
for block_num in {24172600..24172649}; do
    echo "Running block rpc_block_${block_num}..."
    ./target/release/bench-reth -c 20 -i ./block/rpc_block_${block_num}
done
cleanup

# --- Bench 2 worker each 4 gpu ---
echo "Starting Bench 2 worker each 4 gpu..."
./target/release/zisk-coordinator &
COORDINATOR_PID=$!

CUDA_VISIBLE_DEVICES=0,1,2,3 numactl --cpunodebind=0 --membind=0 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23115 -v > ./bench-reth-logs/2-worker-each-4-gpu-worker-0.log 2>&1 &
WORKER_PIDS=($!)

CUDA_VISIBLE_DEVICES=4,5,6,7 numactl --cpunodebind=1 --membind=1 ./target/release/zisk-worker --elf ./stateless-validator-reth --asm-port 23118 -v > ./bench-reth-logs/2-worker-each-4-gpu-worker-1.log 2>&1 &
WORKER_PIDS+=($!)

nvidia-smi dmon -s pucvmet > ./bench-reth-logs/2-worker-each-4-gpu-dmon.log 2>&1 &
DMON_PID=$!

sleep 60
for block_num in {24172600..24172649}; do
    echo "Running block rpc_block_${block_num}..."
    ./target/release/bench-reth -c 20 -i ./block/rpc_block_${block_num}
done
cleanup

echo "All benchmarks completed!"
