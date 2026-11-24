

import { decodeAccountV2 } from "./layouts.js";
import { decodeEventDataV2 } from "./events.js";

export const decodePresetRowV2Typed = (d            )                     => decodeAccountV2("PresetRowV2", d)                      ;
export const decodeLaunchV2Typed = (d            )                  => decodeAccountV2("LaunchV2", d)                   ;
export const decodeHolderPositionTyped = (d            )                        => decodeAccountV2("HolderPosition", d)                         ;
export const decodeHolderBookTyped = (d            )                    => decodeAccountV2("HolderBook", d)                     ;
export const decodeDeadMarkTyped = (d            )                  => decodeAccountV2("DeadMark", d)                   ;

export const decodeEventV2Typed = (d            )                           => decodeEventDataV2(d)                            ;

