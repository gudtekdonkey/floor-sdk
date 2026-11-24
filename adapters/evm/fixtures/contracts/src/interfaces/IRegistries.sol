// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;




interface IRegistries {




    struct PresetRow {
        uint8 id;
        bool enabled;
        bool deleted;
        uint32 rowVersion;
        bytes32 specHash;
        bytes spec;
    }


    struct QuoteEntry {
        bool listed;
        bool enabled;
        uint8 decimals;
        uint256 createFee;
        uint256 usdCentsX64;
        uint64 launches;
        uint64 updatedAt;
    }




    event PresetWritten(uint8 indexed id, uint32 rowVersion, bool enabled, bool deleted);


    event QuoteWritten(address indexed token, bool enabled, uint256 createFee, uint256 usdCentsX64);


    event PriceUpdaterSet(address indexed previous, address indexed updater);






    function addPreset(uint8 id, bytes calldata spec) external;


    function updatePreset(uint8 id, bytes calldata spec) external;

    function setPresetEnabled(uint8 id, bool enabled) external;


    function deletePreset(uint8 id) external;




    function addQuote(address token, uint256 createFee, uint256 usdCentsX64) external;

    function updateQuote(address token, bool enabled, uint256 createFee, uint256 usdCentsX64) external;


    function setPriceUpdater(address updater) external;




    function setQuotePrice(address token, uint256 usdCentsX64) external;



    function nextPresetId() external view returns (uint8);

    function presetRow(uint8 id) external view returns (PresetRow memory);

    function quoteEntry(address token) external view returns (QuoteEntry memory);


    function priceUpdater() external view returns (address);


    function quotes() external view returns (address[] memory);
}
