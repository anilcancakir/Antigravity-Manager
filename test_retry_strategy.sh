#!/bin/bash

# 统一退避策略测试脚本
# 用于验证 Thinking 签名失败和 529 错误的退避行为

API_KEY="sk-7fd8d437a64b4bf8b011fb17945a109d"
BASE_URL="http://127.0.0.1:8045"

echo "========================================="
echo "统一退避策略测试"
echo "========================================="
echo ""

echo "测试 1: 正常请求（无 Thinking）"
echo "预期: 正常返回，无退避日志"
echo "-----------------------------------------"
curl -s -X POST "$BASE_URL/v1/messages" \
  -H "Content-Type: application/json" \
  -H "x-api-key: $API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 50,
    "messages": [{"role": "user", "content": "Say hello"}]
  }' | jq -r '.content[0].text // .error.message // "请求成功"' | head -3

echo ""
echo ""

echo "测试 2: 带 Thinking 的请求"
echo "预期: 如果签名失败，应该看到退避日志"
echo "-----------------------------------------"
curl -s -X POST "$BASE_URL/v1/messages" \
  -H "Content-Type: application/json" \
  -H "x-api-key: $API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-sonnet-4-5-thinking",
    "max_tokens": 100,
    "thinking": {
      "type": "enabled",
      "budget_tokens": 2048
    },
    "messages": [{"role": "user", "content": "What is 1+1?"}]
  }' | jq -r '.content[0].text // .error.message // "请求成功"' | head -3

echo ""
echo ""

echo "========================================="
echo "测试完成！"
echo "========================================="
echo ""
echo "请查看 Tauri 开发服务器的终端输出，寻找以下日志："
echo ""
echo "1. Thinking 签名失败的退避日志："
echo "   [xxx] ⏱️  Retry with fixed delay: status=400, attempt=1/3, waiting=200ms"
echo ""
echo "2. 服务器过载的退避日志："
echo "   [xxx] ⏱️  Retry with exponential backoff: status=529, attempt=1/3, waiting=1000ms"
echo ""
echo "3. 账号轮换决策日志："
echo "   [xxx] Keeping same account for status 529 (server-side issue)"
echo ""
