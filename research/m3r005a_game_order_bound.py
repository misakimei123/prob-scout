from __future__ import annotations

import json
from itertools import product


ELO_SCALE = 400.0
K_FACTOR_PER_GAME = 20.0
RATING_DIFFERENCE_MIN = -1200.0
RATING_DIFFERENCE_MAX = 1200.0
RATING_DIFFERENCE_STEP = 0.1


def elo_probability(team_1_rating: float, team_2_rating: float) -> float:
    return 1.0 / (1.0 + 10.0 ** ((team_2_rating - team_1_rating) / ELO_SCALE))


def series_probability(game_probability: float, best_of: int) -> float:
    if best_of == 3:
        return game_probability**2 * (3.0 - 2.0 * game_probability)
    if best_of == 5:
        return game_probability**3 * (
            10.0 - 15.0 * game_probability + 6.0 * game_probability**2
        )
    raise ValueError(f"unsupported best_of={best_of}")


def valid_series_sequences(best_of: int) -> list[tuple[int, ...]]:
    wins_needed = best_of // 2 + 1
    sequences: list[tuple[int, ...]] = []
    for game_count in range(wins_needed, best_of + 1):
        for sequence in product((0, 1), repeat=game_count):
            team_1_wins = sum(sequence)
            team_2_wins = game_count - team_1_wins
            if max(team_1_wins, team_2_wins) != wins_needed:
                continue

            # 系列赛在任一方达到目标胜局后立即结束，不能枚举已经结束后的伪造小局。
            prefix = sequence[:-1]
            if prefix and max(sum(prefix), len(prefix) - sum(prefix)) >= wins_needed:
                continue
            sequences.append(sequence)
    return sequences


def compute_probability_bounds() -> list[dict[str, object]]:
    bounds: list[dict[str, object]] = []
    step_count = round(
        (RATING_DIFFERENCE_MAX - RATING_DIFFERENCE_MIN) / RATING_DIFFERENCE_STEP
    )
    for previous_best_of in (3, 5):
        maxima = {
            next_best_of: {
                "absolute_probability_delta": 0.0,
                "initial_rating_difference": 0.0,
                "game_sequence": (),
                "signed_probability_delta": 0.0,
            }
            for next_best_of in (3, 5)
        }
        sequences = valid_series_sequences(previous_best_of)
        # 每个 previous BO 合同都必须独立扫描完整 rating 区间，不能复用已耗尽的 generator。
        rating_differences = (
            RATING_DIFFERENCE_MIN + index * RATING_DIFFERENCE_STEP
            for index in range(step_count + 1)
        )
        for rating_difference in rating_differences:
            initial_game_probability = 1.0 / (
                1.0 + 10.0 ** (-rating_difference / ELO_SCALE)
            )
            for sequence in sequences:
                batch_delta = K_FACTOR_PER_GAME * (
                    sum(sequence) - len(sequence) * initial_game_probability
                )
                team_1_rating = rating_difference / 2.0
                team_2_rating = -rating_difference / 2.0
                sequential_delta = 0.0
                for outcome in sequence:
                    game_probability = elo_probability(team_1_rating, team_2_rating)
                    update = K_FACTOR_PER_GAME * (outcome - game_probability)
                    team_1_rating += update
                    team_2_rating -= update
                    sequential_delta += update

                # 只比较上一系列赛的 update 方式；下一场对手固定为中性 rating，避免引入模型选择。
                batch_next_game = elo_probability(
                    rating_difference / 2.0 + batch_delta, 0.0
                )
                sequential_next_game = elo_probability(
                    rating_difference / 2.0 + sequential_delta, 0.0
                )
                for next_best_of in (3, 5):
                    signed_delta = series_probability(
                        sequential_next_game, next_best_of
                    ) - series_probability(batch_next_game, next_best_of)
                    absolute_delta = abs(signed_delta)
                    if (
                        absolute_delta
                        > maxima[next_best_of]["absolute_probability_delta"]
                    ):
                        maxima[next_best_of] = {
                            "absolute_probability_delta": absolute_delta,
                            "initial_rating_difference": rating_difference,
                            "game_sequence": sequence,
                            "signed_probability_delta": signed_delta,
                        }

        for next_best_of, maximum in maxima.items():
            bounds.append(
                {
                    "previous_best_of": previous_best_of,
                    "next_best_of": next_best_of,
                    **maximum,
                }
            )
    return bounds


def main() -> None:
    payload = {
        "contract": {
            "elo_scale": ELO_SCALE,
            "k_factor_per_game": K_FACTOR_PER_GAME,
            "rating_difference_range": [
                RATING_DIFFERENCE_MIN,
                RATING_DIFFERENCE_MAX,
            ],
            "rating_difference_step": RATING_DIFFERENCE_STEP,
        },
        "bounds": compute_probability_bounds(),
    }
    print(json.dumps(payload, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
