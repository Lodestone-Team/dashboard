// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.

export type ExitStatus = { type: "Success", time: bigint, } | { type: "Killed", time: bigint, } | { type: "Error", time: bigint, error_msg: string, };