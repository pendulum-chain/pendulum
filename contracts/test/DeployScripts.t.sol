// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {PEN} from "../src/PEN.sol";
import {MigrationVault} from "../src/MigrationVault.sol";
import {Deploy} from "../script/Deploy.s.sol";

// These run the real scripts against their environment. The environment is
// process-wide and forge runs test functions in parallel, so each script's
// cases share one test function instead of racing on the same variables.
// The scripts broadcast as their caller, so this test contract is the deployer.

contract DeployScriptTest is Test {
    uint256 internal constant SWEEP_FLOOR = 1803859200; // 2027-03-01 00:00 UTC

    Deploy internal script;
    address internal adminSafe = makeAddr("adminSafe");

    function setUp() public {
        script = new Deploy();
        vm.warp(1792000000); // October 2026
    }

    function _setValidEnv() internal {
        vm.setEnv("ADMIN_SAFE", vm.toString(adminSafe));
        vm.setEnv("GUARDIAN_SAFE", vm.toString(makeAddr("guardianSafe")));
        vm.setEnv("ATTESTOR_1", vm.toString(makeAddr("attestor1")));
        vm.setEnv("ATTESTOR_2", vm.toString(makeAddr("attestor2")));
        vm.setEnv("ATTESTOR_3", vm.toString(makeAddr("attestor3")));
        vm.setEnv("ATTESTOR_4", vm.toString(makeAddr("attestor4")));
        vm.setEnv("MAX_ISSUANCE", "150000000000000000000000000");
        vm.setEnv("PER_RELEASE_CAP", "1000000000000000000000000");
        vm.setEnv("DAILY_CAP", "1000000000000000000000000");
        vm.setEnv("EARLIEST_SWEEP_TS", vm.toString(SWEEP_FLOOR));
    }

    function _expectRefused(string memory key, string memory value, string memory reason) internal {
        _setValidEnv();
        vm.setEnv(key, value);
        vm.expectRevert(bytes(reason));
        script.run();
    }

    function test_RunRefusesBadPermanentValuesAndDeploys() public {
        string memory issuance = "MAX_ISSUANCE must be 150M PEN in 18-decimal units";
        _expectRefused("MAX_ISSUANCE", "15000000000000000000000000", issuance); // one zero short
        _expectRefused("MAX_ISSUANCE", "150000000", issuance); // whole PEN

        string memory sweep = "EARLIEST_SWEEP_TS must be a future unix time in seconds, at most two years ahead";
        _expectRefused("EARLIEST_SWEEP_TS", vm.toString(SWEEP_FLOOR * 1000), sweep); // milliseconds
        _expectRefused("EARLIEST_SWEEP_TS", vm.toString(block.timestamp), sweep);

        _setValidEnv();
        (PEN pen, MigrationVault vault) = script.run();
        assertEq(address(vault.token()), address(pen));
        assertEq(pen.totalSupply(), 150_000_000e18);
        assertEq(pen.balanceOf(address(vault)), 150_000_000e18);
        assertEq(vault.earliestSweepTimestamp(), SWEEP_FLOOR);
        assertEq(vault.admin(), address(this), "interim admin until the Safe accepts");
        assertEq(vault.pendingAdmin(), adminSafe);
    }
}
