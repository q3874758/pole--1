"""
PoLE 代币经济模型模拟脚本
版本: 1.0
日期: 2026年2月

功能:
1. 模拟代币通胀曲线
2. 模拟燃烧机制效果
3. 模拟节点收益
4. 敏感性分析
"""

import math
from dataclasses import dataclass
from typing import List, Dict
import json


@dataclass
class TokenConfig:
    """代币配置"""
    total_supply: int = 1_000_000_000  # 10亿
    initial_inflation: float = 0.20    # 20%
    decay_factor: float = 0.5           # 每2年减半
    burn_percentage: float = 0.25        # 交易燃烧25%


@dataclass
class SimulationResult:
    """模拟结果"""
    year: int
    inflation_rate: float
    tokens_minted: float
    tokens_burned: float
    net_inflation: float
    circulating_supply: float
    node_reward: float
    validator_apr: float


class InflationModel:
    """通胀模型"""
    
    def __init__(self, config: TokenConfig):
        self.config = config
    
    def get_inflation_rate(self, year: int) -> float:
        """
        计算年度通胀率
        
        公式: InflationRate(year) = Initial × (0.5)^(floor((year-1)/2))
        """
        if year <= 0:
            return 0
        
        exponent = (year - 1) // 2
        rate = self.config.initial_inflation * (self.config.decay_factor ** exponent)
        return rate
    
    def get_yearly_mintage(self, year: int, current_supply: float) -> float:
        """计算年度铸造量"""
        rate = self.get_inflation_rate(year)
        return current_supply * rate


class BurnModel:
    """燃烧模型"""
    
    def __init__(self, config: TokenConfig):
        self.config = config
    
    def calculate_transaction_burn(
        self, 
        daily_transactions: int, 
        avg_fee: float,
        days: int = 365
    ) -> float:
        """计算交易燃烧"""
        yearly_fees = daily_transactions * avg_fee * days
        return yearly_fees * self.config.burn_percentage
    
    def calculate_punishment_burn(self, violations: int, avg_slash: float) -> float:
        """计算惩罚性燃烧"""
        return violations * avg_slash


class NodeEconomics:
    """节点经济模型"""
    
    def __init__(
        self,
        node_count: int,
        avg_stake: float,
        block_reward: float
    ):
        self.node_count = node_count
        self.avg_stake = avg_stake
        self.block_reward = block_reward
        self.blocks_per_year = 365 * 24 * 60 * 60 / 6  # 约6秒一个块
    
    def calculate_yearly_reward(self) -> float:
        """计算年度节点总奖励"""
        return self.blocks_per_year * self.block_reward
    
    def calculate_validator_apr(
        self, 
        yearly_reward: float, 
        total_staked: float
    ) -> float:
        """计算验证节点年化收益率"""
        if total_staked <= 0:
            return 0
        return (yearly_reward / total_staked) * 100


class TokenomicsSimulator:
    """代币经济模拟器"""
    
    def __init__(self, config: TokenConfig = None):
        self.config = config or TokenConfig()
        self.inflation_model = InflationModel(self.config)
        self.burn_model = BurnModel(self.config)
        self.results: List[SimulationResult] = []
    
    def run_simulation(
        self,
        years: int = 10,
        daily_transactions: int = 50_000,
        avg_fee: float = 0.1,
        node_count: int = 10_000,
        avg_stake: float = 15_000,
        block_reward: float = 50.0,
        transaction_growth: float = 0.2  # 年增长率
    ) -> List[SimulationResult]:
        """运行模拟"""
        
        circulating = 0.0  # 初始流通量
        self.results = []
        
        node_economics = NodeEconomics(node_count, avg_stake, block_reward)
        
        for year in range(1, years + 1):
            # 1. 计算通胀
            inflation_rate = self.inflation_model.get_inflation_rate(year)
            tokens_minted = self.inflation_model.get_yearly_mintage(year, circulating)
            
            # 2. 计算燃烧
            # 交易燃烧
            tx_burn = self.burn_model.calculate_transaction_burn(
                daily_transactions, avg_fee
            )
            # 假设惩罚燃烧为交易燃烧的5%
            punishment_burn = tx_burn * 0.05
            total_burn = tx_burn + punishment_burn
            
            # 3. 净通胀
            net_inflation = tokens_minted - total_burn
            circulating += net_inflation
            
            # 4. 节点奖励
            node_reward = node_economics.calculate_yearly_reward()
            total_staked = node_count * avg_stake
            validator_apr = node_economics.calculate_validator_apr(
                node_reward, total_staked
            )
            
            # 记录结果
            result = SimulationResult(
                year=year,
                inflation_rate=inflation_rate * 100,
                tokens_minted=tokens_minted,
                tokens_burned=total_burn,
                net_inflation=net_inflation,
                circulating_supply=circulating,
                node_reward=node_reward,
                validator_apr=validator_apr
            )
            self.results.append(result)
            
            # 更新参数
            daily_transactions = int(daily_transactions * (1 + transaction_growth))
            node_count = int(node_count * 1.2)  # 节点增长20%
        
        return self.results
    
    def print_results(self):
        """打印模拟结果"""
        print("\n" + "=" * 100)
        print("PoLE 代币经济模型模拟结果")
        print("=" * 100)
        
        print(f"\n{'年份':<6} {'通胀率':<10} {'铸造量':<15} {'燃烧量':<15} {'净通胀':<15} {'流通量':<15} {'节点奖励':<15} {'APR':<8}")
        print("-" * 100)
        
        for r in self.results:
            print(f"{r.year:<6} {r.inflation_rate:>6.2f}%  {r.tokens_minted:>12,.0f}  {r.tokens_burned:>12,.0f}  {r.net_inflation:>12,.0f}  {r.circulating_supply:>12,.0f}  {r.node_reward:>12,.0f}  {r.validator_apr:>5.1f}%")
    
    def export_json(self, filename: str):
        """导出JSON"""
        data = [
            {
                "year": r.year,
                "inflation_rate": round(r.inflation_rate, 2),
                "tokens_minted": round(r.tokens_minted, 2),
                "tokens_burned": round(r.tokens_burned, 2),
                "net_inflation": round(r.net_inflation, 2),
                "circulating_supply": round(r.circulating_supply, 2),
                "node_reward": round(r.node_reward, 2),
                "validator_apr": round(r.validator_apr, 2)
            }
            for r in self.results
        ]
        
        with open(filename, 'w', encoding='utf-8') as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
        
        print(f"\n结果已导出到: {filename}")


class SensitivityAnalysis:
    """敏感性分析"""
    
    def __init__(self, base_config: TokenConfig = None):
        self.base_config = base_config or TokenConfig()
    
    def analyze_inflation_impact(self):
        """分析通胀率变化的影响"""
        print("\n" + "=" * 80)
        print("敏感性分析: 通胀率变化")
        print("=" * 80)
        
        scenarios = [
            ("低通胀 (-5%)", 0.15),
            ("基准", 0.20),
            ("高通胀 (+5%)", 0.25),
        ]
        
        for name, inflation in scenarios:
            simulator = TokenomicsSimulator(
                TokenConfig(initial_inflation=inflation)
            )
            simulator.run_simulation(years=5, daily_transactions=50_000)
            
            # 第5年数据
            year5 = simulator.results[4]
            print(f"\n{name} (通胀率: {inflation*100:.0f}%)")
            print(f"  第5年流通量: {year5.circulating_supply:,.0f}")
            print(f"  第5年净通胀: {year5.net_inflation:,.0f}")
            print(f"  第5年APR: {year5.validator_apr:.1f}%")
    
    def analyze_transaction_impact(self):
        """分析交易量变化的影响"""
        print("\n" + "=" * 80)
        print("敏感性分析: 交易量变化")
        print("=" * 80)
        
        scenarios = [
            ("低活跃 (10万/天)", 10_000),
            ("基准 (50万/天)", 50_000),
            ("高活跃 (100万/天)", 100_000),
        ]
        
        for name, txs in scenarios:
            simulator = TokenomicsSimulator()
            simulator.run_simulation(years=5, daily_transactions=txs)
            
            year5 = simulator.results[4]
            print(f"\n{name}")
            print(f"  第5年燃烧量: {year5.tokens_burned:,.0f}")
            print(f"  第5年净通胀: {year5.net_inflation:,.0f}")
            print(f"  实际通胀率: {year5.net_inflation/year5.circulating_supply*100:.2f}%")


def scenario_comparison():
    """不同场景对比"""
    
    print("\n" + "=" * 80)
    print("场景对比分析")
    print("=" * 80)
    
    scenarios = [
        {
            "name": "熊市",
            "daily_txs": 10_000,
            "tx_growth": -0.05,
            "node_growth": 0.05,
            "fee": 0.05
        },
        {
            "name": "正常",
            "daily_txs": 50_000,
            "tx_growth": 0.20,
            "node_growth": 0.20,
            "fee": 0.10
        },
        {
            "name": "牛市",
            "daily_txs": 100_000,
            "tx_growth": 0.50,
            "node_growth": 0.50,
            "fee": 0.20
        },
    ]
    
    for scenario in scenarios:
        print(f"\n--- {scenario['name']} 场景 ---")
        
        simulator = TokenomicsSimulator()
        simulator.run_simulation(
            years=5,
            daily_transactions=scenario["daily_txs"],
            avg_fee=scenario["fee"],
            transaction_growth=scenario["tx_growth"]
        )
        
        year5 = simulator.results[4]
        
        print(f"  第5年流通量: {year5.circulating_supply:,.0f}")
        print(f"  第5年节点奖励: {year5.node_reward:,.0f}")
        print(f"  第5年验证者APR: {year5.validator_apr:.1f}%")


def main():
    """主函数"""
    # 1. 基础模拟
    print("\n" + "#" * 80)
    print("# 基础模拟")
    print("#" * 80)
    
    simulator = TokenomicsSimulator()
    simulator.run_simulation(
        years=10,
        daily_transactions=50_000,
        avg_fee=0.1,
        node_count=10_000,
        avg_stake=15_000,
        block_reward=50.0
    )
    simulator.print_results()
    
    # 2. 导出JSON
    simulator.export_json("D:\\PoLE\\tokenomics_simulation.json")
    
    # 3. 敏感性分析
    sensitivity = SensitivityAnalysis()
    sensitivity.analyze_inflation_impact()
    sensitivity.analyze_transaction_impact()
    
    # 4. 场景对比
    scenario_comparison()
    
    print("\n" + "=" * 80)
    print("模拟完成!")
    print("=" * 80)


if __name__ == "__main__":
    main()
