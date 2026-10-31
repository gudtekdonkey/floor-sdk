

import { hopIn } from "./path.js";

        

        

                                                                                                            

                               

export function routeInputs(q            )           {
  return q.path.hops.map(hopIn);
}
