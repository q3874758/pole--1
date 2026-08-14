#!/bin/bash
# PoLE 主网启动脚本

echo "🚀 启动 PoLE 主网节点..."

# 检查 Go 环境
if ! command -v go &> /dev/null; then
    echo "❌ Go 未安装"
    exit 1
fi

# 编译
echo "📦 编译中..."
cd "$(dirname "$0")/.."
go build -o pole-node ./cmd/pole

if [ $? -eq 0 ]; then
    echo "✅ 编译成功"
else
    echo "❌ 编译失败"
    exit 1
fi

# 启动节点
echo "🔗 启动主网节点..."
./pole-node mainnet \
    --datadir ./data \
    --http.port 8545 \
    --ws.port 8546 \
    --maxpeers 50 \
    --bootnodes "$BOOTNODES"
