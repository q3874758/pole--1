package types

import "math/big"

const (
	weightScalePPM     = uint64(1_000_000)
	rewardAdjustScale  = uint64(1_000_000_000_000)
	basisPointsDivisor = uint64(10_000)
)

func ComputePlayerHourWeight(effectivePlaySeconds uint64, gameWeightPPM uint32) uint64 {
	if effectivePlaySeconds == 0 || gameWeightPPM == 0 {
		return 0
	}
	return effectivePlaySeconds * uint64(gameWeightPPM)
}

func ComputePlayerHourReward(hourlyRewardPool uint64, playerWeight uint64, totalWeight uint64) uint64 {
	if hourlyRewardPool == 0 || playerWeight == 0 || totalWeight == 0 {
		return 0
	}
	return hourlyRewardPool * playerWeight / totalWeight
}

func AdjustedHourlyReward(baseHourlyReward uint64, targetNetworkWeight uint64, previousNetworkWeight uint64, rewardAdjustmentCapBps uint32) uint64 {
	if baseHourlyReward == 0 {
		return 0
	}
	if targetNetworkWeight == 0 || previousNetworkWeight == 0 {
		return baseHourlyReward
	}

	capBps := rewardAdjustmentCapBps
	if uint64(capBps) > basisPointsDivisor {
		capBps = uint32(basisPointsDivisor)
	}

	lowerBound := mulDiv(baseHourlyReward, basisPointsDivisor-uint64(capBps), basisPointsDivisor)
	upperBound := mulDiv(baseHourlyReward, basisPointsDivisor+uint64(capBps), basisPointsDivisor)

	scaledRatio := big.NewInt(0).Mul(new(big.Int).SetUint64(targetNetworkWeight), new(big.Int).SetUint64(rewardAdjustScale))
	scaledRatio.Div(scaledRatio, new(big.Int).SetUint64(previousNetworkWeight))

	ratioSqrt := new(big.Int).Sqrt(scaledRatio)
	adjustedBig := big.NewInt(0).Mul(new(big.Int).SetUint64(baseHourlyReward), ratioSqrt)
	adjustedBig.Div(adjustedBig, new(big.Int).SetUint64(weightScalePPM))

	adjusted := adjustedBig.Uint64()
	if adjusted < lowerBound {
		adjusted = lowerBound
	}
	if adjusted > upperBound {
		adjusted = upperBound
	}
	if adjusted == 0 {
		return 1
	}
	return adjusted
}

func mulDiv(a uint64, b uint64, divisor uint64) uint64 {
	result := big.NewInt(0).Mul(new(big.Int).SetUint64(a), new(big.Int).SetUint64(b))
	result.Div(result, new(big.Int).SetUint64(divisor))
	return result.Uint64()
}
