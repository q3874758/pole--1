"""
PoLE GVS (Game Value Score) 计算演示代码
版本: 1.0
日期: 2026年2月
"""

from dataclasses import dataclass
from typing import List, Dict, Optional
from enum import Enum
import math


class Tier(Enum):
    """游戏数据可信度层级"""
    TIER_1 = 1      # 核心验证层 - 有公开API
    TIER_2 = 2      # 增强验证层 - 第三方数据
    TIER_3 = 3      # 社区验证层 - 无公开数据


@dataclass
class GameData:
    """游戏数据"""
    game_id: str
    game_name: str
    tier: Tier
    current_players: int      # 当前在线玩家数
    peak_players: int        # 峰值玩家数
    avg_players_30d: float   # 30天平均玩家数
    node_coverage: int       # 节点覆盖数量
    timestamp: int


@dataclass
class GVSResult:
    """GVS计算结果"""
    game_id: str
    base_glv: float
    tier_coefficient: float
    time_decay: float
    coverage_bonus: float
    final_gvs: float


class GVSCalculator:
    """GVS (Game Value Score) 计算器"""
    
    # Tier 系数
    TIER_COEFFICIENTS = {
        Tier.TIER_1: 1.0,
        Tier.TIER_2: (0.3 + 0.6) / 2,  # 0.45
        Tier.TIER_3: (0.05 + 0.15) / 2,  # 0.1
    }
    
    # 时间衰减参数
    DECAY_HALF_LIFE = 30  # 30天半衰期
    
    # 覆盖加成参数
    COVERAGE_THRESHOLD = 10  # 基础阈值
    COVERAGE_BONUS_MAX = 0.5  # 最大加成50%
    
    def __init__(self, target_tngv: float = 1_000_000):
        """
        初始化
        target_tngv: 目标网络总GVS
        """
        self.target_tngv = target_tngv
    
    def calculate_base_glv(self, data: GameData) -> float:
        """
        计算基础游戏列表价值 (Base_GLV)
        
        公式: sqrt(当前玩家 × 峰值玩家) × log(历史平均 + 1)
        """
        if data.current_players <= 0:
            return 0.0
        
        # 几何平均
        geometric_mean = math.sqrt(data.current_players * data.peak_players)
        
        # 对数调整
        log_avg = math.log(data.avg_players_30d + 1)
        
        base_glv = geometric_mean * log_avg
        return base_glv
    
    def calculate_tier_coefficient(self, tier: Tier) -> float:
        """计算Tier系数"""
        return self.TIER_COEFFICIENTS.get(tier, 0.1)
    
    def calculate_time_decay(self, avg_players: float, current_players: int) -> float:
        """
        计算时间衰减因子
        
        比较当前活跃度与历史平均
        - 当前 > 历史: 衰减因子 > 1 (热度上升)
        - 当前 < 历史: 衰减因子 < 1 (热度下降)
        """
        if avg_players <= 0:
            return 1.0
        
        ratio = current_players / avg_players
        
        # 使用sigmoid函数平滑
        time_decay = 1.0 + math.tanh((ratio - 1.0) * 2) * 0.3
        
        return max(0.5, min(1.5, time_decay))  # 限制在0.5-1.5
    
    def calculate_coverage_bonus(self, node_coverage: int) -> float:
        """
        计算节点覆盖加成
        
        覆盖节点越多，数据越可信，给予奖励加成
        """
        if node_coverage <= self.COVERAGE_THRESHOLD:
            return 0.0
        
        # 线性增长到最大值
        bonus = (node_coverage - self.COVERAGE_THRESHOLD) / 100 * self.COVERAGE_BONUS_MAX
        return min(bonus, self.COVERAGE_BONUS_MAX)
    
    def calculate_gvs(self, data: GameData) -> GVSResult:
        """
        计算游戏的GVS
        
        公式: GVS = Base_GLV × Tier_Coefficient × Time_Decay × (1 + Coverage_Bonus)
        """
        # 1. 计算基础价值
        base_glv = self.calculate_base_glv(data)
        
        # 2. 计算Tier系数
        tier_coefficient = self.calculate_tier_coefficient(data.tier)
        
        # 3. 计算时间衰减
        time_decay = self.calculate_time_decay(
            data.avg_players_30d, 
            data.current_players
        )
        
        # 4. 计算覆盖加成
        coverage_bonus = self.calculate_coverage_bonus(data.node_coverage)
        
        # 5. 最终GVS
        final_gvs = base_glv * tier_coefficient * time_decay * (1 + coverage_bonus)
        
        return GVSResult(
            game_id=data.game_id,
            base_glv=round(base_glv, 2),
            tier_coefficient=tier_coefficient,
            time_decay=round(time_decay, 4),
            coverage_bonus=round(coverage_bonus, 4),
            final_gvs=round(final_gvs, 2)
        )


class NetworkGVS:
    """网络总GVS管理"""
    
    def __init__(self):
        self.game_gvs: Dict[str, float] = {}
    
    def add_game_gvs(self, gvs_result: GVSResult):
        """添加游戏的GVS"""
        self.game_gvs[gvs_result.game_id] = gvs_result.final_gvs
    
    def get_total_gvs(self) -> float:
        """获取网络总GVS"""
        return sum(self.game_gvs.values())
    
    def calculate_reward(self, node_gvs: float, epoch_reward: float = 100_000) -> float:
        """
        计算节点奖励
        
        公式: Node_Reward = (Node_GVS / Total_GVS) × Epoch_Reward
        """
        total = self.get_total_gvs()
        if total <= 0:
            return 0.0
        
        return (node_gvs / total) * epoch_reward


# 示例数据
def demo():
    """演示GVS计算"""
    
    calculator = GVSCalculator(target_tngv=1_000_000)
    network = NetworkGVS()
    
    # 示例游戏数据
    games = [
        GameData(
            game_id="730",
            game_name="CS2",
            tier=Tier.TIER_1,
            current_players=1500000,
            peak_players=1800000,
            avg_players_30d=1200000,
            node_coverage=500,
            timestamp=1700000000
        ),
        GameData(
            game_id="570",
            game_name="Dota 2",
            tier=Tier.TIER_1,
            current_players=800000,
            peak_players=1000000,
            avg_players_30d=750000,
            node_coverage=300,
            timestamp=1700000000
        ),
        GameData(
            game_id="1091500",
            game_name="Palworld",
            tier=Tier.TIER_2,
            current_players=50000,
            peak_players=200000,
            avg_players_30d=80000,
            node_coverage=50,
            timestamp=1700000000
        ),
        GameData(
            game_id="custom_001",
            game_name="独立小游戏",
            tier=Tier.TIER_3,
            current_players=100,
            peak_players=500,
            avg_players_30d=200,
            node_coverage=15,
            timestamp=1700000000
        ),
    ]
    
    print("=" * 60)
    print("PoLE GVS 计算演示")
    print("=" * 60)
    
    # 计算每个游戏的GVS
    for game in games:
        result = calculator.calculate_gvs(game)
        network.add_game_gvs(result)
        
        print(f"\n游戏: {game.game_name} (Tier {game.tier.value})")
        print(f"  当前玩家: {game.current_players:,}")
        print(f"  峰值玩家: {game.peak_players:,}")
        print(f"  节点覆盖: {game.node_coverage}")
        print(f"  ─────────────────────")
        print(f"  Base_GLV:       {result.base_glv:,.2f}")
        print(f"  Tier系数:       {result.tier_coefficient}")
        print(f"  时间衰减:       {result.time_decay}")
        print(f"  覆盖加成:       {result.coverage_bonus}")
        print(f"  ─────────────────────")
        print(f"  最终GVS:        {result.final_gvs:,.2f}")
    
    # 网络总GVS
    print("\n" + "=" * 60)
    print(f"网络总GVS: {network.get_total_gvs():,.2f}")
    print("=" * 60)
    
    # 节点奖励示例
    print("\n节点奖励示例:")
    node_gvs = 50000  # 假设节点贡献了50000 GVS
    reward = network.calculate_reward(node_gvs)
    print(f"  节点贡献GVS: {node_gvs:,}")
    print(f"  Epoch奖励: {reward:,.2f} $POLE")


if __name__ == "__main__":
    demo()
