// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;

import {IFloorAmm} from "./IFloorAmm.sol";








interface IFloorZap {

    event Zapped(
        address indexed sender,
        address indexed to,
        uint256 indexed launchId,
        address tokenIn,
        uint256 amountIn,
        uint256 quoteIn,
        uint256 tokensOut,
        uint256 refundIn,
        uint256 refundQuote
    );

    error NotRouter();
    error SwapFailed(bytes reason);
    error SwapDataNotEmpty();
    error QuoteShort();
    error SlippageExceeded();
    error Expired();
    error BadValue();
    error Reentrancy();


    function floorAmm() external view returns (IFloorAmm);


    function router() external view returns (address);





    function buyWithEth(
        uint256 launchId,
        bytes calldata swapData,
        uint256 minQuoteIn,
        uint256 minTokensOut,
        address to,
        uint256 deadline
    ) external payable returns (uint256 tokensOut, uint256 quoteIn);


    function buyWithToken(
        uint256 launchId,
        address tokenIn,
        uint256 amountIn,
        bytes calldata swapData,
        uint256 minQuoteIn,
        uint256 minTokensOut,
        address to,
        uint256 deadline
    ) external returns (uint256 tokensOut, uint256 quoteIn);
}
