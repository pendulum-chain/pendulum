// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {TimelockController} from "@openzeppelin/contracts/governance/TimelockController.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {PEN} from "../src/PEN.sol";
import {MigrationVault} from "../src/MigrationVault.sol";
import {PENGovernor} from "../src/PENGovernor.sol";
import {Deploy} from "../script/Deploy.s.sol";
import {DeployGovernance} from "../script/DeployGovernance.s.sol";

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

// Answers getThreshold() like a Safe. Immutable, so an account delegating to
// it answers the same.
contract SafeStub {
    uint256 internal immutable threshold;

    constructor(uint256 threshold_) {
        threshold = threshold_;
    }

    function getThreshold() external view returns (uint256) {
        return threshold;
    }
}

contract DeployGovernanceScriptTest is Test {
    uint256 internal constant SUPPLY = 150_000_000e18;

    DeployGovernance internal script;
    PEN internal pen;
    MigrationVault internal vault;
    // The dedicated canceller Safe, separate from the vault's guardian Safe.
    address internal canceller = address(new SafeStub(2));
    address internal guardian = address(new SafeStub(2));

    function setUp() public {
        script = new DeployGovernance();
        address[] memory attestors = new address[](3);
        attestors[0] = makeAddr("a0");
        attestors[1] = makeAddr("a1");
        attestors[2] = makeAddr("a2");
        vault = new MigrationVault(address(this), guardian, attestors, 2, 1e6, 1e24, 1e24, block.timestamp + 365 days);
        pen = new PEN(address(vault), SUPPLY);
        vault.setToken(IERC20(address(pen)));
    }

    function _setValidEnv() internal {
        vm.setEnv("TEST_DEPLOY", "false");
        vm.setEnv("PEN_TOKEN", vm.toString(address(pen)));
        vm.setEnv("VAULT_ADDRESS", vm.toString(address(vault)));
        vm.setEnv("TIMELOCK_DELAY", "172800");
        vm.setEnv("VOTING_DELAY", "86400");
        vm.setEnv("VOTING_PERIOD", "432000");
        vm.setEnv("PROPOSAL_THRESHOLD", "1000000000000000000000"); // 1,000 PEN
        vm.setEnv("QUORUM_FRACTION", "2");
        vm.setEnv("QUORUM_FLOOR", "100000000000000000000000"); // 100,000 PEN
        vm.setEnv("TIMELOCK_CANCELLER", vm.toString(canceller));
    }

    function _expectRefused(string memory key, string memory value, string memory reason) internal {
        _setValidEnv();
        vm.setEnv(key, value);
        vm.expectRevert(bytes(reason));
        script.run();
    }

    function test_RunValidatesEveryParameterAndWiresRoles() public {
        string memory notToken = "PEN_TOKEN is not the migration vault's token";
        _expectRefused("PEN_TOKEN", vm.toString(address(vault)), notToken);
        _expectRefused("PEN_TOKEN", vm.toString(address(new PEN(makeAddr("other"), SUPPLY))), notToken);

        _expectRefused("TIMELOCK_DELAY", "0", "TIMELOCK_DELAY is below 48h");
        _expectRefused("TIMELOCK_DELAY", "172799", "TIMELOCK_DELAY is below 48h");
        _expectRefused("TIMELOCK_DELAY", "172800000", "TIMELOCK_DELAY is above 30 days (seconds expected)");
        _expectRefused("VOTING_DELAY", "86400000", "VOTING_DELAY is above 30 days (seconds expected)");
        // Would have been narrowed to 432000 by a bare uint32 cast.
        _expectRefused("VOTING_PERIOD", "4295399296", "VOTING_PERIOD is above 30 days (seconds expected)");

        string memory threshold = "PROPOSAL_THRESHOLD must be 18-decimal units, 1 PEN to total supply";
        _expectRefused("PROPOSAL_THRESHOLD", "1000", threshold); // whole PEN
        _expectRefused("PROPOSAL_THRESHOLD", vm.toString(SUPPLY + 1), threshold);
        string memory floor = "QUORUM_FLOOR must be 18-decimal units, 1 PEN to total supply";
        _expectRefused("QUORUM_FLOOR", "100000", floor); // whole PEN
        _expectRefused("QUORUM_FLOOR", "0", floor);
        _expectRefused("QUORUM_FLOOR", vm.toString(SUPPLY + 1), floor);

        _expectRefused(
            "TIMELOCK_CANCELLER", "", "TIMELOCK_CANCELLER is required: no veto during the timelock delay otherwise"
        );
        _expectRefused("TIMELOCK_CANCELLER", vm.toString(address(this)), "TIMELOCK_CANCELLER must not be the deployer");
        string memory notPenOrVault = "TIMELOCK_CANCELLER must not be PEN or the vault";
        _expectRefused("TIMELOCK_CANCELLER", vm.toString(address(pen)), notPenOrVault);
        _expectRefused("TIMELOCK_CANCELLER", vm.toString(address(vault)), notPenOrVault);
        _expectRefused(
            "TIMELOCK_CANCELLER", vm.toString(guardian), "TIMELOCK_CANCELLER must be separate from the guardian"
        );
        string memory notSafe = "TIMELOCK_CANCELLER is not a Safe on this chain (getThreshold() failed)";
        _expectRefused("TIMELOCK_CANCELLER", vm.toString(makeAddr("codeless")), notSafe);
        _expectRefused("TIMELOCK_CANCELLER", vm.toString(address(new PEN(makeAddr("other"), SUPPLY))), notSafe);
        _expectRefused("TIMELOCK_CANCELLER", vm.toString(address(new SafeStub(0))), notSafe);
        address delegated = makeAddr("delegatedEoa");
        vm.etch(delegated, abi.encodePacked(hex"ef0100", canceller)); // EIP-7702 delegation to a Safe
        _expectRefused("TIMELOCK_CANCELLER", vm.toString(delegated), notSafe);

        vm.chainId(8453);
        _expectRefused("TEST_DEPLOY", "true", "TEST_DEPLOY is refused on Base mainnet");
        vm.chainId(31337);

        _setValidEnv();
        (TimelockController timelock, PENGovernor governor) = script.run();
        bytes32[4] memory roles = [
            timelock.DEFAULT_ADMIN_ROLE(), timelock.PROPOSER_ROLE(), timelock.CANCELLER_ROLE(), timelock.EXECUTOR_ROLE()
        ];
        for (uint256 i = 0; i < roles.length; i++) {
            assertFalse(timelock.hasRole(roles[i], address(this)), "deployer keeps no timelock role");
        }
        assertTrue(timelock.hasRole(timelock.CANCELLER_ROLE(), canceller));
        assertTrue(timelock.hasRole(timelock.CANCELLER_ROLE(), address(governor)));
        assertTrue(timelock.hasRole(timelock.PROPOSER_ROLE(), address(governor)));
        assertTrue(timelock.hasRole(timelock.EXECUTOR_ROLE(), address(0)));
        assertTrue(timelock.hasRole(timelock.DEFAULT_ADMIN_ROLE(), address(timelock)));
        assertEq(timelock.getMinDelay(), 2 days);
        assertEq(address(governor.token()), address(pen));
        assertEq(governor.votingDelay(), 1 days);
        assertEq(governor.votingPeriod(), 5 days);
        assertEq(governor.proposalThreshold(), 1_000e18);
        assertEq(governor.quorumFloor(), 100_000e18);

        // Testnet drills: a short delay, no floor and the guardian key, not a
        // Safe, as canceller.
        address guardianKey = makeAddr("guardianKey");
        vault.setGuardian(guardianKey);
        _setValidEnv();
        vm.setEnv("TEST_DEPLOY", "true");
        vm.setEnv("TIMELOCK_DELAY", "180");
        vm.setEnv("QUORUM_FLOOR", "0");
        vm.setEnv("TIMELOCK_CANCELLER", vm.toString(guardianKey));
        (timelock,) = script.run();
        assertEq(timelock.getMinDelay(), 180);
        assertTrue(timelock.hasRole(timelock.CANCELLER_ROLE(), guardianKey));
    }
}
