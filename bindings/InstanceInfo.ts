// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { Game } from "./Game";
import type { InstanceState } from "./InstanceState";
import type { InstanceUuid } from "./InstanceUuid";

export interface InstanceInfo { uuid: InstanceUuid, name: string, game_type: Game, description: string, port: number, creation_time: bigint, path: string, auto_start: boolean, restart_on_crash: boolean, state: InstanceState, player_count: number | null, max_player_count: number | null, }