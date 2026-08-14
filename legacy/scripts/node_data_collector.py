"""
PoLE 节点数据采集脚本原型
版本: 1.0
日期: 2026年2月

功能:
1. 检测游戏进程
2. 调用 Steam API 获取玩家数据
3. 生成活跃度证明
"""

import hashlib
import hmac
import json
import time
import subprocess
import platform
import requests
from dataclasses import dataclass, asdict
from typing import List, Dict, Optional
from pathlib import Path


# Steam API 配置
STEAM_API_KEY = "YOUR_STEAM_API_KEY"  # 需要从 Steam 申请
STEAM_API_BASE = "https://api.steampowered.com"
STEAM_APP_LIST_URL = "https://api.steampowered.com/ISteamApps/GetAppList/v2/"
STEAM_PLAYERS_URL = "https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/"


@dataclass
class GameProcess:
    """游戏进程信息"""
    process_name: str
    app_id: str
    game_name: str
    start_time: float


@dataclass
class PlayerCount:
    """玩家数量数据"""
    app_id: str
    player_count: int
    timestamp: int
    result: int  # 1 = success


@dataclass
class ActivityProof:
    """活跃度证明"""
    node_id: str
    app_id: str
    player_count: int
    timestamp: int
    hardware_hash: str
    signature: str


class ProcessDetector:
    """游戏进程检测器"""
    
    # 常见游戏进程映射到 Steam App ID
    GAME_PROCESS_MAP = {
        # Windows
        "cs2.exe": "730",           # Counter-Strike 2
        "dota2.exe": "570",         # Dota 2
        "valorant.exe": "1094520",  # Valorant
        "pubg.exe": "578080",      # PUBG
        "apexlegends.exe": "1172470",  # Apex Legends
        "fortnite.exe": "0",        # Fortnite (需要额外处理)
        "leagueoflegends.exe": "0", # LoL
        "genshin.exe": "0",         # 原神
        
        # Linux (如果有)
        "cs2": "730",
        "dota2": "570",
    }
    
    def __init__(self):
        self.platform = platform.system()
    
    def get_running_games(self) -> List[GameProcess]:
        """获取当前运行的游戏进程"""
        games = []
        
        try:
            if self.platform == "Windows":
                games = self._get_windows_games()
            elif self.platform == "Linux":
                games = self._get_linux_games()
            elif self.platform == "Darwin":
                games = self._get_macos_games()
        except Exception as e:
            print(f"检测进程出错: {e}")
        
        return games
    
    def _get_windows_games(self) -> List[GameProcess]:
        """Windows 平台进程检测"""
        games = []
        
        try:
            # 使用 tasklist 获取进程列表
            result = subprocess.run(
                ["tasklist", "/FO", "CSV", "/NH"],
                capture_output=True,
                text=True,
                timeout=10
            )
            
            running_processes = result.stdout.lower()
            
            for process_name, app_id in self.GAME_PROCESS_MAP.items():
                if process_name.lower() in running_processes:
                    game = GameProcess(
                        process_name=process_name,
                        app_id=app_id,
                        game_name=self._get_game_name(app_id),
                        start_time=time.time()
                    )
                    games.append(game)
                    
        except Exception as e:
            print(f"Windows 进程检测错误: {e}")
        
        return games
    
    def _get_linux_games(self) -> List[GameProcess]:
        """Linux 平台进程检测"""
        games = []
        
        try:
            result = subprocess.run(
                ["ps", "aux"],
                capture_output=True,
                text=True,
                timeout=10
            )
            
            running_processes = result.stdout.lower()
            
            for process_name, app_id in self.GAME_PROCESS_MAP.items():
                if process_name.lower() in running_processes:
                    game = GameProcess(
                        process_name=process_name,
                        app_id=app_id,
                        game_name=self._get_game_name(app_id),
                        start_time=time.time()
                    )
                    games.append(game)
                    
        except Exception as e:
            print(f"Linux 进程检测错误: {e}")
        
        return games
    
    def _get_macos_games(self) -> List[GameProcess]:
        """macOS 平台进程检测"""
        # 类似 Linux
        return self._get_linux_games()
    
    def _get_game_name(self, app_id: str) -> str:
        """获取游戏名称（简化版）"""
        # 实际应该从本地缓存或 Steam API 获取
        names = {
            "730": "Counter-Strike 2",
            "570": "Dota 2",
            "1094520": "Valorant",
            "578080": "PUBG",
            "1172470": "Apex Legends",
        }
        return names.get(app_id, f"Game {app_id}")


class SteamAPIClient:
    """Steam API 客户端"""
    
    def __init__(self, api_key: str = None):
        self.api_key = api_key or STEAM_API_KEY
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})
        
        # 游戏名称缓存
        self.game_cache: Dict[str, str] = {}
    
    def get_player_count(self, app_id: str) -> Optional[PlayerCount]:
        """获取游戏当前玩家数"""
        if not app_id or app_id == "0":
            return None
            
        try:
            url = f"{STEAM_PLAYERS_URL}/?appid={app_id}&key={self.api_key}"
            response = self.session.get(url, timeout=10)
            data = response.json()
            
            if data.get("response", {}).get("result") == 1:
                return PlayerCount(
                    app_id=app_id,
                    player_count=data["response"]["player_count"],
                    timestamp=int(time.time()),
                    result=1
                )
                
        except Exception as e:
            print(f"获取玩家数失败 [{app_id}]: {e}")
        
        return None
    
    def get_multiple_player_counts(self, app_ids: List[str]) -> List[PlayerCount]:
        """批量获取玩家数"""
        results = []
        
        for app_id in app_ids:
            result = self.get_player_count(app_id)
            if result:
                results.append(result)
            time.sleep(0.5)  # 避免请求过快
        
        return results
    
    def get_app_list(self) -> Dict[str, str]:
        """获取 Steam 游戏列表"""
        try:
            response = self.session.get(STEAM_APP_LIST_URL, timeout=30)
            data = response.json()
            
            apps = data.get("applist", {}).get("apps", {})
            self.game_cache = {str(app["appid"]): app["name"] for app in apps}
            return self.game_cache
            
        except Exception as e:
            print(f"获取游戏列表失败: {e}")
            return {}


class ActivityProofGenerator:
    """活跃度证明生成器"""
    
    def __init__(self, node_private_key: str = None):
        self.node_private_key = node_private_key or "default_node_key"
    
    def generate_hardware_hash(self) -> str:
        """生成硬件指纹哈希"""
        # 简化实现：使用机器名和用户名
        info = f"{platform.node()}-{platform.processor()}-{platform.machine()}"
        return hashlib.sha256(info.encode()).hexdigest()[:16]
    
    def sign_data(self, data: str) -> str:
        """签名数据（简化实现）"""
        key = self.node_private_key.encode()
        signature = hmac.new(key, data.encode(), hashlib.sha256).hexdigest()
        return signature
    
    def generate_proof(
        self, 
        node_id: str, 
        player_count: PlayerCount
    ) -> ActivityProof:
        """生成活跃度证明"""
        
        # 构建数据字符串
        data_str = f"{node_id}:{player_count.app_id}:{player_count.player_count}:{player_count.timestamp}"
        
        # 生成硬件哈希
        hardware_hash = self.generate_hardware_hash()
        
        # 签名
        signature = self.sign_data(data_str)
        
        return ActivityProof(
            node_id=node_id,
            app_id=player_count.app_id,
            player_count=player_count.player_count,
            timestamp=player_count.timestamp,
            hardware_hash=hardware_hash,
            signature=signature
        )
    
    def verify_proof(self, proof: ActivityProof) -> bool:
        """验证活跃度证明"""
        data_str = f"{proof.node_id}:{proof.app_id}:{proof.player_count}:{proof.timestamp}"
        expected_signature = self.sign_data(data_str)
        return hmac.compare_digest(proof.signature, expected_signature)


class NodeCollector:
    """节点数据采集器"""
    
    def __init__(self, node_id: str, api_key: str = None):
        self.node_id = node_id
        self.detector = ProcessDetector()
        self.api_client = SteamAPIClient(api_key)
        self.proof_generator = ActivityProofGenerator()
    
    def collect(self) -> List[ActivityProof]:
        """采集活跃度数据"""
        proofs = []
        
        print(f"\n=== 节点 {self.node_id} 开始采集 ===")
        
        # 1. 检测运行的游戏
        running_games = self.detector.get_running_games()
        print(f"检测到 {len(running_games)} 个游戏进程")
        
        # 2. 获取各游戏玩家数
        app_ids = [game.app_id for game in running_games if game.app_id != "0"]
        player_counts = self.api_client.get_multiple_player_counts(app_ids)
        
        # 3. 生成活跃度证明
        for pc in player_counts:
            proof = self.proof_generator.generate_proof(self.node_id, pc)
            proofs.append(proof)
            
            print(f"  - AppID: {proof.app_id}, 玩家: {proof.player_count:,}")
        
        print(f"生成了 {len(proofs)} 个活跃度证明")
        
        return proofs
    
    def submit_proofs(self, proofs: List[ActivityProof]) -> bool:
        """
        提交证明到网络（模拟）
        实际应该发送到 PoLE 网络的节点
        """
        print("\n提交证明到网络...")
        
        # 验证所有证明
        valid_count = 0
        for proof in proofs:
            if self.proof_generator.verify_proof(proof):
                valid_count += 1
        
        print(f"验证通过: {valid_count}/{len(proofs)}")
        print("提交成功!")
        
        return valid_count > 0


def demo():
    """演示数据采集流程"""
    
    print("=" * 60)
    print("PoLE 节点数据采集演示")
    print("=" * 60)
    
    # 初始化节点
    node_id = "node_001"
    collector = NodeCollector(node_id)
    
    # 采集数据
    proofs = collector.collect()
    
    # 提交数据
    if proofs:
        collector.submit_proofs(proofs)
    
    # 打印证明详情
    print("\n" + "=" * 60)
    print("活跃度证明详情:")
    print("=" * 60)
    
    for proof in proofs:
        print(f"""
App ID:     {proof.app_id}
玩家数:    {proof.player_count:,}
时间戳:    {proof.timestamp}
硬件哈希:  {proof.hardware_hash}
签名:      {proof.signature[:32]}...
        """)


if __name__ == "__main__":
    demo()
