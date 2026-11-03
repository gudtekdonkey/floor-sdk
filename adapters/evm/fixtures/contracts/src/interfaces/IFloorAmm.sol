// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;

import {IRegistries} from "./IRegistries.sol";










interface IFloorAmm is IRegistries {





    struct CreateParams {
        uint8 presetId;
        address quote;
        string name;
        string symbol;


        uint256 creatorBuy;
        uint256 creatorBuyMinTokens;



        bool sameAddressEverywhere;
        bytes32 creatorSalt;


        uint256 quoteUsdCentsX64;


        uint16 teamSlotRateBps;
    }


    struct BuyResult {
        uint256 tokensOut;
        uint256 spent;
        uint256 refund;
        uint256 priceAfterX64;
        bool graduatedNow;
        bool endedNow;

        bool budgetHit;
    }


    struct Owed {
        uint128 creator;
        uint128 creatorRef;
        uint128 teamSlot;
        uint128 traderRef;
        uint128 holder;
        uint128 extra;
    }


    struct LaunchView {
        address token;
        address quote;
        address creator;
        uint8 presetId;
        uint32 presetVersion;
        bytes32 presetHash;
        bool graduated;
        uint64 createdAt;
        uint64 createdBlock;
        uint64 creatorUnlockAt;
        uint256 supply;
        uint256 sOut;
        uint256 creatorLocked;
        uint256 floor;
        uint256 cushion;
        uint256 p0X64;
        uint256 priceX64;
        uint256 floorX64;
        uint16 slotRateBps;
    }





    event LaunchCreated(
        uint256 indexed launchId,
        address indexed token,
        address indexed quote,
        address creator,
        uint8 presetId,
        uint32 presetVersion,
        bytes32 presetHash,
        uint256 p0X64,
        uint256 raise,
        uint256 windowTokens,
        uint256 creatorBuyTokens,
        uint256 createFee,
        string name,
        string symbol,
        string uri,


        bool listed,
        uint256 quoteUsdCentsX64,
        uint256 nativeCreateFee
    );





    event Trade(
        uint256 indexed launchId,
        address indexed trader,
        address indexed recipient,
        bool isBuy,
        uint256 quoteGross,
        uint256 quoteNet,
        uint256 refund,
        uint256 tokens,
        uint256[7] fees,
        uint256 traderRef,
        uint256[4] split,
        uint256[5] credits,
        uint256 priceBeforeX64,
        uint256 priceAfterX64,
        uint256 floorAfterX64,
        uint256 floor,
        uint256 cushion,
        uint256 reserveTokens,
        uint256 sOut,
        uint256 floorToken,
        uint256 msPlatform,
        uint256 msFloor,
        uint256 tailPay,
        uint256 tailToFloor,
        uint256 tailTokens,
        uint256 tailFromFloor,
        uint256 tailFromMoney,
        uint256 msHolder,
        uint256 polOut,
        uint256 polPay,
        uint256 polTokens,
        uint256 polGross,
        uint256 holderFee,
        uint256 liveBuyPriceX64,
        uint256 polMoney
    );



    event Graduated(
        uint256 indexed launchId, uint256 windowPriceX64, uint256 priceX64, uint256 windowRaised, uint256 windowTokens
    );



    event PotsSwept(uint256 indexed launchId, uint256 amount, uint8 reason);


    event Claimed(
        address indexed wallet,
        address indexed sender,
        address indexed quote,
        uint8 types,
        uint256 creator,
        uint256 creatorRef,
        uint256 teamSlot,
        uint256 traderRef,
        uint256 holder,
        uint256 extra,
        uint256 amount
    );


    event ClaimSkipped(address indexed wallet, address indexed quote, uint256 amount);

    event PlatformWithdrawn(address indexed quote, address indexed to, uint256 amount, uint256 owedAfter);


    event HolderWithdrawn(address indexed quote, address indexed to, uint256 amount, uint256 owedAfter);


    event TreasurySet(address indexed previous, address indexed treasury);


    event NativeCreateFeeSet(uint256 fee);


    event MetadataBaseSet(string base);

    event ConfigWritten(
        address authority,
        address pendingAuthority,
        address timelock,
        address referralAuthority,
        bool createPaused,
        address holderDest
    );

    event TeamSlotOpened(uint256 indexed launchId, uint16 rateBps, uint256 price);

    event TeamSlotMember(
        uint256 indexed launchId,
        uint8 index,
        bytes32 key,
        uint8 kind,
        uint8 state,
        uint256 paid,
        uint256 requested,
        uint256 bought,
        uint32 boughtPpm
    );


    event HandleBound(bytes32 indexed handleId, address indexed wallet, uint256 moved);

    event HandleOwedMoved(bytes32 indexed handleId, address indexed wallet, uint256 moved);


    event TraderRefOpened(address indexed trader, address[3] tiers);



    event TraderRefCleared(address indexed trader);



    event CreatorBuyUnlocked(uint256 indexed launchId, address indexed creator, uint256 tokens, uint256[10] members, uint256 paid);


    event TeamBuyClaimed(uint256 indexed launchId, address indexed wallet, uint8 place, uint256 tokens);


    event PayoutOwed(address indexed wallet, address indexed quote, uint256 amount);


    event PayoutClaimed(address indexed wallet, address indexed sender, address indexed quote, address to, uint256 amount);


    event QuoteCapSet(address indexed token, uint128 cap);





    error NotAuthority();
    error NotTimelock();

    error NotReferralAuthority();
    error PresetInvalid();
    error PresetIdNotNext();
    error PresetDeleted();
    error PresetDisabled();
    error QuoteNotListed();
    error QuoteListed();
    error QuoteDisabled();
    error QuoteTransferShort();
    error CreatesPaused();
    error CreatorBuyTooLarge();
    error SlippageExceeded();
    error NothingToDo();
    error Overflow();
    error MathFailed(uint8 code);
    error UnknownLaunch();
    error Locked();
    error OverOwed();
    error UnknownClaimType();
    error TeamSlotRateOverMax();
    error TeamSlotFull();
    error TeamSlotPaid();
    error TeamSlotExists();
    error TeamSlotNoRequest();
    error SellTooLarge();
    error BadArgument();
    error BadValue();
    error Expired();
    error NativeTransferFailed();
    error Reentrancy();
    error NotCreator();
    error HandleAlreadyBound();
    error NothingToUnlock();

    error BadSignature();

    error TraderRefSet();

    error TreasuryUnset();

    error NotPriceUpdater();

    error MetadataBaseUnset();


    error MetadataBaseInvalid();

    error QuoteRowCannotOpen(uint8 presetId, address quote);

    error ReferralSigExpired();

    error BadReferralSig();

    error ClaimsShort();

    error HolderDestUnset();

    error RoleNotHeld();

    error BurnTooLarge();






    function initialize(address authority, address timelock, address referralAuthority, address holderDest) external;


    function transferAuthority(address newAuthority) external;
    function acceptAuthority() external;


    function setTimelock(address timelock) external;


    function setCreatePaused(bool paused) external;

    function setReferralAuthority(address referralAuthority) external;


    function setHolderDest(address holderDest) external;



    function withdrawHolder(address quote, uint256 amount) external returns (uint256 paid);



    function withdrawPlatform(address quote, uint256 amount) external returns (uint256 paid);



    function setTreasury(address treasury) external;



    function setNativeCreateFee(uint256 fee) external;




    function setMetadataBase(string calldata base) external;












    function createLaunch(CreateParams calldata p)
        external
        payable
        returns (uint256 launchId, address token, uint256 creatorTokens);





    function buy(uint256 launchId, uint256 amountIn, uint256 minTokensOut, address to, uint256 deadline)
        external
        payable
        returns (BuyResult memory r);



    function buyFromBalance(uint256 launchId, uint256 minTokensOut, address to, address refundTo, uint256 deadline)
        external
        returns (BuyResult memory r);


    function sell(uint256 launchId, uint256 tokensIn, uint256 minQuoteOut, address to, uint256 deadline)
        external
        returns (uint256 quoteOut);



    function sellWithPermit(
        uint256 launchId,
        uint256 tokensIn,
        uint256 minQuoteOut,
        address to,
        uint256 deadline,
        uint256 permitDeadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external returns (uint256 quoteOut);


    function sellFromBalance(uint256 launchId, uint256 minQuoteOut, address to, uint256 deadline)
        external
        returns (uint256 quoteOut);





    function unlockCreatorBuy(uint256 launchId) external returns (uint256 tokens);







    function teamSlotAdd(uint256 launchId, bytes32 key, uint8 kind, uint256 amount, bool payNow) external payable;







    function bindHandle(
        bytes32 handleId,
        address wallet,
        address[] calldata quotes_,
        uint256 deadline,
        bytes calldata authSig,
        bytes calldata walletSig
    ) external;




    function moveHandleOwed(bytes32 handleId, address[] calldata quotes_) external returns (uint256 moved);










    function setTraderRef(
        address trader,
        address[3] calldata tiers,
        uint256 deadline,
        bytes calldata authSig,
        bytes calldata traderSig
    ) external;



    function clearTraderRef(address trader) external;












    function claim(address wallet, uint256 types, uint256[] calldata launchIds, address[] calldata quotes_)
        external
        returns (uint256[] memory paid);





    function NATIVE() external pure returns (address);

    function authority() external view returns (address);
    function pendingAuthority() external view returns (address);
    function timelock() external view returns (address);
    function referralAuthority() external view returns (address);
    function holderDest() external view returns (address);

    function treasury() external view returns (address);

    function nativeCreateFee() external view returns (uint256);

    function metadataBase() external view returns (string memory);
    function createPaused() external view returns (bool);


    function launchCount() external view returns (uint256);

    function launchQuote(uint256 launchId) external view returns (address);
    function launchToken(uint256 launchId) external view returns (address);

    function launchOf(address token) external view returns (uint256 launchId);
    function launchState(uint256 launchId) external view returns (LaunchView memory);


    function quoteBuy(uint256 launchId, uint256 amountIn, address to) external view returns (BuyResult memory r);

    function quoteSell(uint256 launchId, uint256 tokensIn, address to) external view returns (uint256 quoteOut);


    function owedOf(address wallet, address quote) external view returns (Owed memory);
    function owedOfMany(address wallet, address[] calldata quotes_) external view returns (Owed[] memory);

    function handleOwed(bytes32 handleId, address quote) external view returns (uint256);
    function handleWallet(bytes32 handleId) external view returns (address);
    function traderRef(address trader) external view returns (address[3] memory tiers);


    function platformOwed(address quote) external view returns (uint256);


    function accounted(address quote) external view returns (uint256);

    function holderBucket(address quote) external view returns (uint256);







    function claimPayoutTo(address quote, address to) external returns (uint256 paid);


    function payoutOwed(address wallet, address quote) external view returns (uint256);



    function quoteRisk(address token)
        external
        view
        returns (uint256 accounted_, uint256 held, uint128 cap, uint32 shortIns, uint32 failedOuts, uint64 lastFailBlock);


    function setQuoteCap(address token, uint128 cap) external;



    function checkPreset(bytes calldata spec) external view returns (bool ok);



    function floorTokenAccrued(address quote) external view returns (uint256);




    function predictToken(address creator, bytes32 creatorSalt) external view returns (address);









    event TeamShareBought(
        uint256 indexed launchId,
        address indexed member,
        uint8 place,
        uint256 spent,
        uint256 refund,
        uint256 tokens,
        uint256 shareLeft,
        uint256 boughtOwed,
        uint64 day,
        bool firstToday
    );


    event TeamShareBoughtClaimed(uint256 indexed launchId, address indexed wallet, uint256 tokens);


    error NotTeamMember();



    function teamShareBuy(uint256 launchId, uint256 amount, uint256 minTokensOut, uint256 deadline)
        external
        returns (BuyResult memory);



    function teamShareOf(uint256 launchId, address wallet)
        external
        view
        returns (uint256 claimable, uint256 boughtOwed, uint64 lastDay);
}
