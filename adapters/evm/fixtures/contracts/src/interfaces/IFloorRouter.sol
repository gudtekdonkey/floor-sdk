// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;

import {IFloorAmm} from "./IFloorAmm.sol";






interface IFloorRouter {

    struct Sell {
        uint256[] path;
        uint256 tokensIn;
        uint256[] minQuoteOut;
        address tokenOut;
        uint256 swapIn;
        bytes swapData;
        uint256 minOut;
        address to;
        uint256 deadline;
    }



    event Sold(
        address indexed sender,
        address indexed to,
        uint256 indexed launchId,
        uint256 tokensIn,
        uint256 hops,
        address tokenOut,
        uint256 quoteOut,
        uint256 amountOut,
        uint256 refundQuote
    );


    struct Buy {
        uint256[] path;
        address tokenIn;
        uint256 amountIn;
        bytes swapData;
        uint256 minQuoteIn;
        uint256[] minTokensOut;
        address to;
        uint256 deadline;
    }



    event Bought(
        address indexed sender,
        address indexed to,
        uint256 indexed launchId,
        address tokenIn,
        uint256 amountIn,
        uint256 hops,
        uint256 quoteIn,
        uint256 tokensOut,
        uint256 refundIn
    );

    error NotRouter();
    error SwapFailed(bytes reason);
    error SwapDataNotEmpty();
    error BadPath();
    error QuoteShort();
    error SlippageExceeded();
    error Expired();
    error BadValue();
    error Reentrancy();


    function floorAmm() external view returns (IFloorAmm);


    function router() external view returns (address);


    function sell(Sell calldata s) external returns (uint256 amountOut, uint256 quoteOut);


    function sellWithPermit(Sell calldata s, uint256 permitDeadline, uint8 v, bytes32 r, bytes32 ss)
        external
        returns (uint256 amountOut, uint256 quoteOut);



    function buy(Buy calldata b) external payable returns (uint256 tokensOut, uint256 quoteIn);




    function swap(Sell calldata s, Buy calldata b) external returns (uint256 tokensOut);
}
