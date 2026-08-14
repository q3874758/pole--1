# PoLE 测试指南

## 概述

本文档描述 PoLE 项目的测试策略、测试工具和最佳实践。

## 测试层次

### 1. 单元测试

测试单个函数或方法的功能。

**Go 单元测试**:
```bash
# 运行所有测试
go test ./...

# 运行特定包的测试
go test ./governance -v

# 运行特定测试
go test ./governance -run TestCreateProposal -v

# 查看覆盖率
go test ./governance -cover

# 生成覆盖率报告
go test ./governance -coverprofile=coverage.out
go tool cover -html=coverage.out
```

**Rust 单元测试**:
```bash
# 运行所有测试
cargo test

# 运行特定包的测试
cd data/availability
cargo test

# 显示输出
cargo test -- --nocapture

# 运行特定测试
cargo test test_store_and_retrieve
```

### 2. 集成测试

测试多个模块之间的交互。

**创建集成测试**:
```go
// tests/integration/governance_test.go
package integration

import (
	"testing"
	"pole-core/governance"
	"pole-core/core/state"
)

func TestGovernanceIntegration(t *testing.T) {
	// Setup
	chainState := state.NewChainState("test")
	gm := governance.NewGovernanceManager(...)
	
	// Test workflow
	// ...
}
```

### 3. E2E 测试

测试完整的用户场景。

**示例 E2E 测试**:
```bash
# 启动测试网络
./scripts/start-testnet.sh

# 运行 E2E 测试
go test ./tests/e2e -v

# 清理
./scripts/stop-testnet.sh
```

## 测试工具

### Go 测试工具

#### 1. 标准库 testing
```go
import "testing"

func TestExample(t *testing.T) {
	// Arrange
	input := "test"
	
	// Act
	result := SomeFunction(input)
	
	// Assert
	if result != expected {
		t.Errorf("expected %v, got %v", expected, result)
	}
}
```

#### 2. Testify (推荐)
```go
import (
	"testing"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestWithTestify(t *testing.T) {
	result := SomeFunction()
	
	// Assert (继续执行)
	assert.Equal(t, expected, result)
	assert.NotNil(t, result)
	
	// Require (失败则停止)
	require.NoError(t, err)
	require.NotEmpty(t, result)
}
```

#### 3. Mock 生成
```bash
# 安装 mockgen
go install github.com/golang/mock/mockgen@latest

# 生成 mock
mockgen -source=interface.go -destination=mock_interface.go
```

### Rust 测试工具

#### 1. 内置测试框架
```rust
#[cfg(test)]
mod tests {
	use super::*;
	
	#[test]
	fn test_example() {
		let result = some_function();
		assert_eq!(result, expected);
	}
	
	#[test]
	#[should_panic]
	fn test_panic() {
		panic_function();
	}
}
```

#### 2. 属性测试 (proptest)
```rust
use proptest::prelude::*;

proptest! {
	#[test]
	fn test_property(x in 0..100) {
		assert!(some_property(x));
	}
}
```

## 测试最佳实践

### 1. 测试命名

**Go**:
```go
// 格式: Test<FunctionName>_<Scenario>_<ExpectedResult>
func TestTransfer_InsufficientBalance_ReturnsError(t *testing.T) {}
func TestTransfer_ValidAmount_Success(t *testing.T) {}
```

**Rust**:
```rust
#[test]
fn test_transfer_insufficient_balance_returns_error() {}

#[test]
fn test_transfer_valid_amount_success() {}
```

### 2. AAA 模式

```go
func TestExample(t *testing.T) {
	// Arrange - 准备测试数据
	input := "test"
	expected := "result"
	
	// Act - 执行被测试的代码
	result := SomeFunction(input)
	
	// Assert - 验证结果
	if result != expected {
		t.Errorf("expected %v, got %v", expected, result)
	}
}
```

### 3. 表驱动测试

```go
func TestAdd(t *testing.T) {
	tests := []struct {
		name     string
		a, b     int
		expected int
	}{
		{"positive numbers", 1, 2, 3},
		{"negative numbers", -1, -2, -3},
		{"mixed", 1, -1, 0},
	}
	
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := Add(tt.a, tt.b)
			if result != tt.expected {
				t.Errorf("expected %d, got %d", tt.expected, result)
			}
		})
	}
}
```

### 4. 测试辅助函数

```go
// test_helpers.go
package mypackage

import (
	"testing"
	"math/big"
)

// POLE 创建 POLE 代币金额
func POLE(amount int64) *big.Int {
	return new(big.Int).Mul(big.NewInt(amount), pow10(18))
}

// AssertBalance 断言余额
func AssertBalance(t *testing.T, contract *TokenContract, addr string, expected *big.Int) {
	t.Helper()
	balance := contract.BalanceOf(addr)
	if balance.Cmp(expected) != 0 {
		t.Errorf("expected balance %s, got %s", expected.String(), balance.String())
	}
}
```

### 5. 清理资源

```go
func TestWithCleanup(t *testing.T) {
	// Setup
	resource := setupResource()
	
	// Cleanup
	t.Cleanup(func() {
		resource.Close()
	})
	
	// Test
	// ...
}
```

### 6. 并发测试

```go
func TestConcurrent(t *testing.T) {
	t.Parallel() // 允许并行运行
	
	// Test concurrent access
	done := make(chan bool, 10)
	for i := 0; i < 10; i++ {
		go func() {
			// Concurrent operation
			done <- true
		}()
	}
	
	// Wait for all goroutines
	for i := 0; i < 10; i++ {
		<-done
	}
}
```

## 测试覆盖率

### 生成覆盖率报告

```bash
# Go
go test ./... -coverprofile=coverage.out
go tool cover -html=coverage.out -o coverage.html

# Rust
cargo tarpaulin --out Html
```

### 覆盖率目标

- **关键模块**: 80%+ 覆盖率
  - governance
  - contracts (token, staking)
  - core/executor
  
- **一般模块**: 60%+ 覆盖率
  - rpc
  - wallet
  
- **工具模块**: 40%+ 覆盖率
  - utils
  - helpers

## 性能测试

### Go Benchmark

```go
func BenchmarkTransfer(b *testing.B) {
	tc := NewTokenContract(nil)
	from := "addr1"
	to := "addr2"
	amount := big.NewInt(100)
	
	tc.Mint(nil, from, big.NewInt(1000000))
	
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tc.Transfer(nil, from, to, amount)
	}
}
```

运行 benchmark:
```bash
go test -bench=. -benchmem
go test -bench=BenchmarkTransfer -benchtime=10s
```

### Rust Benchmark

```rust
#[bench]
fn bench_transfer(b: &mut Bencher) {
	let tc = TokenContract::new();
	
	b.iter(|| {
		tc.transfer("from", "to", 100);
	});
}
```

## Mock 和 Stub

### 接口 Mock

```go
// Interface
type TokenInterface interface {
	Transfer(from, to string, amount *big.Int) error
	BalanceOf(addr string) *big.Int
}

// Mock implementation
type MockToken struct {
	TransferFunc  func(from, to string, amount *big.Int) error
	BalanceOfFunc func(addr string) *big.Int
}

func (m *MockToken) Transfer(from, to string, amount *big.Int) error {
	if m.TransferFunc != nil {
		return m.TransferFunc(from, to, amount)
	}
	return nil
}

func (m *MockToken) BalanceOf(addr string) *big.Int {
	if m.BalanceOfFunc != nil {
		return m.BalanceOfFunc(addr)
	}
	return big.NewInt(0)
}

// Usage in test
func TestWithMock(t *testing.T) {
	mock := &MockToken{
		BalanceOfFunc: func(addr string) *big.Int {
			return big.NewInt(1000)
		},
	}
	
	balance := mock.BalanceOf("test")
	assert.Equal(t, big.NewInt(1000), balance)
}
```

## 测试数据

### 测试夹具

```go
// fixtures/genesis.json
{
	"chain_id": "test-chain",
	"accounts": [
		{"address": "test1", "balance": "1000"},
		{"address": "test2", "balance": "2000"}
	]
}

// 加载测试数据
func LoadTestGenesis(t *testing.T) *Genesis {
	data, err := os.ReadFile("fixtures/genesis.json")
	require.NoError(t, err)
	
	var genesis Genesis
	err = json.Unmarshal(data, &genesis)
	require.NoError(t, err)
	
	return &genesis
}
```

## CI/CD 集成

### GitHub Actions

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test-go:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions/setup-go@v2
        with:
          go-version: '1.24'
      - run: go test ./... -v -cover
      
  test-rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - run: cargo test --all
```

## 故障排查

### 常见问题

#### 1. 测试超时
```go
func TestLongRunning(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping long-running test")
	}
	
	// Long test...
}
```

运行:
```bash
go test -short  # 跳过长测试
go test -timeout 30s  # 设置超时
```

#### 2. 竞态条件
```bash
go test -race ./...
```

#### 3. 内存泄漏
```bash
go test -memprofile=mem.prof
go tool pprof mem.prof
```

## 测试清单

在提交代码前，确保:

- [ ] 所有测试通过
- [ ] 新代码有测试覆盖
- [ ] 测试命名清晰
- [ ] 无竞态条件
- [ ] 清理了测试资源
- [ ] 更新了文档

## 参考资源

- [Go Testing](https://golang.org/pkg/testing/)
- [Rust Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Testify](https://github.com/stretchr/testify)
- [Mockgen](https://github.com/golang/mock)
