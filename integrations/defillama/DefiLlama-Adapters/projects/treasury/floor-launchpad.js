const { treasuryExports } = require('../helper/treasury')

// FLOOR launchpad (https://floor.supply). The platform treasury (FLOOR's cut of the skim and of each raise) and the team
// treasury (the FLOOR Platform fee). On EVM the same two addresses on every chain; on Solana the floor_hook program's
// ["platform_treasury"] and ["team_treasury"] accounts, which own the token accounts the fees land in.
const PLATFORM_TREASURY = '0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c'
const TEAM_TREASURY = '0x062B6c421E37579728877337B47Cf528AE4d49bb'
const evm = { owners: [PLATFORM_TREASURY, TEAM_TREASURY] }

module.exports = treasuryExports({
  ethereum: evm,
  base: evm,
  bsc: evm,
  robinhood: evm,
  arc: evm,
  solana: {
    owners: [
      'B8SRexSGm2p2nJFpMmVQPB9PepHSXeB3oQMzyaqrsGvC', // floor_hook ["platform_treasury"]
      'CqQXgHT6MsUmEaXgwuUZURwRvfF7w5wkcD1BzWNt7vi9', // floor_hook ["team_treasury"]
    ],
  },
})
