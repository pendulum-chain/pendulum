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
// process-wide, forge runs test functions in parallel and both scripts read
// TEST_DEPLOY, so all cases share one test function instead of racing on the
// same variables. The scripts broadcast as their caller, so this test
// contract is the deployer.

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

// Answers every call with the given raw bytes.
contract RawAnswerStub {
    bytes internal answer;

    constructor(bytes memory answer_) {
        answer = answer_;
    }

    fallback(bytes calldata) external returns (bytes memory) {
        return answer;
    }
}

contract DeployScriptsTest is Test {
    uint256 internal constant SWEEP_FLOOR = 1803859200; // 2027-03-01 00:00 UTC
    uint256 internal constant SUPPLY = 150_000_000e18;

    Deploy internal deployScript;
    address internal adminSafe = makeAddr("adminSafe");

    DeployGovernance internal governanceScript;
    PEN internal pen;
    MigrationVault internal vault;
    // The dedicated canceller Safe, separate from the vault's guardian Safe,
    // at the lowest threshold the script accepts.
    address internal canceller = address(new SafeStub(2));
    address internal guardian = address(new SafeStub(2));

    function setUp() public {
        vm.warp(1792000000); // October 2026
        deployScript = new Deploy();
        governanceScript = new DeployGovernance();
        address[] memory attestors = new address[](3);
        attestors[0] = makeAddr("a0");
        attestors[1] = makeAddr("a1");
        attestors[2] = makeAddr("a2");
        vault = new MigrationVault(address(this), guardian, attestors, 2, 1e6, 1e24, 1e24, block.timestamp + 365 days);
        pen = new PEN(address(vault), SUPPLY);
        vault.setToken(IERC20(address(pen)));
    }

    function test_RunScriptsRefuseBadValuesAndDeploy() public {
        _runDeployCases();
        _runGovernanceCases();
    }

    function _setValidDeployEnv() internal {
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

    function _expectDeployRefused(string memory key, string memory value, string memory reason) internal {
        _setValidDeployEnv();
        vm.setEnv(key, value);
        vm.expectRevert(bytes(reason));
        deployScript.run();
    }

    function _runDeployCases() internal {
        string memory issuance = "MAX_ISSUANCE must be 150M PEN in 18-decimal units";
        _expectDeployRefused("MAX_ISSUANCE", "15000000000000000000000000", issuance); // one zero short
        _expectDeployRefused("MAX_ISSUANCE", "150000000", issuance); // whole PEN

        string memory sweep = "EARLIEST_SWEEP_TS must be a future unix time in seconds, at most two years ahead";
        _expectDeployRefused("EARLIEST_SWEEP_TS", vm.toString(SWEEP_FLOOR * 1000), sweep); // milliseconds
        _expectDeployRefused("EARLIEST_SWEEP_TS", vm.toString(block.timestamp), sweep);

        _setValidDeployEnv();
        (PEN newPen, MigrationVault newVault) = deployScript.run();
        assertEq(address(newVault.token()), address(newPen));
        assertEq(newPen.totalSupply(), 150_000_000e18);
        assertEq(newPen.balanceOf(address(newVault)), 150_000_000e18);
        assertEq(newVault.earliestSweepTimestamp(), SWEEP_FLOOR);
        assertEq(newVault.admin(), address(this), "interim admin until the Safe accepts");
        assertEq(newVault.pendingAdmin(), adminSafe);
    }

    function _setValidGovernanceEnv() internal {
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

    function _expectGovernanceRefused(string memory key, string memory value, string memory reason) internal {
        _setValidGovernanceEnv();
        vm.setEnv(key, value);
        vm.expectRevert(bytes(reason));
        governanceScript.run();
    }

    function _runGovernanceCases() internal {
        string memory notToken = "PEN_TOKEN is not the migration vault's token";
        _expectGovernanceRefused("PEN_TOKEN", vm.toString(address(vault)), notToken);
        _expectGovernanceRefused("PEN_TOKEN", vm.toString(address(new PEN(makeAddr("other"), SUPPLY))), notToken);

        _expectGovernanceRefused("TIMELOCK_DELAY", "0", "TIMELOCK_DELAY is below 48h");
        _expectGovernanceRefused("TIMELOCK_DELAY", "172799", "TIMELOCK_DELAY is below 48h");
        _expectGovernanceRefused("TIMELOCK_DELAY", "172800000", "TIMELOCK_DELAY is above 30 days (seconds expected)");
        _expectGovernanceRefused("VOTING_DELAY", "1", "VOTING_DELAY is below 1h (seconds expected)"); // days
        _expectGovernanceRefused("VOTING_DELAY", "3599", "VOTING_DELAY is below 1h (seconds expected)");
        _expectGovernanceRefused("VOTING_DELAY", "86400000", "VOTING_DELAY is above 30 days (seconds expected)");
        _expectGovernanceRefused("VOTING_PERIOD", "5", "VOTING_PERIOD is below 1 day (seconds expected)"); // days
        _expectGovernanceRefused("VOTING_PERIOD", "86399", "VOTING_PERIOD is below 1 day (seconds expected)");
        // Would have been narrowed to 432000 by a bare uint32 cast.
        _expectGovernanceRefused("VOTING_PERIOD", "4295399296", "VOTING_PERIOD is above 30 days (seconds expected)");

        string memory threshold = "PROPOSAL_THRESHOLD must be 18-decimal units, 1 PEN to total supply";
        _expectGovernanceRefused("PROPOSAL_THRESHOLD", "1000", threshold); // whole PEN
        _expectGovernanceRefused("PROPOSAL_THRESHOLD", vm.toString(SUPPLY + 1), threshold);
        _expectGovernanceRefused("QUORUM_FRACTION", "0", "QUORUM_FRACTION must be above 0");
        string memory floor = "QUORUM_FLOOR must be 18-decimal units, 1 PEN to total supply";
        _expectGovernanceRefused("QUORUM_FLOOR", "100000", floor); // whole PEN
        _expectGovernanceRefused("QUORUM_FLOOR", "0", floor);
        _expectGovernanceRefused("QUORUM_FLOOR", vm.toString(SUPPLY + 1), floor);

        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER", "", "TIMELOCK_CANCELLER is required: no veto during the timelock delay otherwise"
        );
        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER", vm.toString(address(this)), "TIMELOCK_CANCELLER must not be the deployer"
        );
        string memory notPenOrVault = "TIMELOCK_CANCELLER must not be PEN or the vault";
        _expectGovernanceRefused("TIMELOCK_CANCELLER", vm.toString(address(pen)), notPenOrVault);
        _expectGovernanceRefused("TIMELOCK_CANCELLER", vm.toString(address(vault)), notPenOrVault);
        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER", vm.toString(guardian), "TIMELOCK_CANCELLER must be separate from the guardian"
        );
        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER",
            vm.toString(makeAddr("codeless")),
            "TIMELOCK_CANCELLER has no code on this chain (create the Safe first)"
        );
        address delegated = makeAddr("delegatedEoa");
        vm.etch(delegated, abi.encodePacked(hex"ef0100", canceller)); // EIP-7702 delegation to a Safe
        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER", vm.toString(delegated), "TIMELOCK_CANCELLER is an EIP-7702 delegated EOA, not a Safe"
        );
        string memory notSafe = "TIMELOCK_CANCELLER is not a Safe (getThreshold() failed)";
        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER", vm.toString(address(new PEN(makeAddr("other"), SUPPLY))), notSafe
        );
        // Answers that are not exactly one word, whatever their first bytes say.
        _expectGovernanceRefused("TIMELOCK_CANCELLER", vm.toString(address(new RawAnswerStub(hex"03"))), notSafe);
        bytes memory twoWords = abi.encode(uint256(3), uint256(3));
        _expectGovernanceRefused("TIMELOCK_CANCELLER", vm.toString(address(new RawAnswerStub(twoWords))), notSafe);
        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER",
            vm.toString(address(new SafeStub(0))),
            "TIMELOCK_CANCELLER Safe has threshold 0 (not set up)"
        );
        _expectGovernanceRefused(
            "TIMELOCK_CANCELLER", vm.toString(address(new SafeStub(1))), "TIMELOCK_CANCELLER Safe threshold is below 2"
        );

        vm.chainId(8453);
        _expectGovernanceRefused("TEST_DEPLOY", "true", "TEST_DEPLOY is refused on Base mainnet");
        vm.chainId(31337);

        _setValidGovernanceEnv();
        (TimelockController timelock, PENGovernor governor) = governanceScript.run();
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
        assertEq(governor.quorumNumerator(), 2);
        assertEq(governor.quorumFloor(), 100_000e18);

        // Testnet drills: short delays and voting, no quorum fraction or floor
        // and the guardian key, not a Safe, as canceller.
        address guardianKey = makeAddr("guardianKey");
        vault.setGuardian(guardianKey);
        _setValidGovernanceEnv();
        vm.setEnv("TEST_DEPLOY", "true");
        vm.setEnv("TIMELOCK_DELAY", "180");
        vm.setEnv("VOTING_DELAY", "60");
        vm.setEnv("VOTING_PERIOD", "240");
        vm.setEnv("QUORUM_FRACTION", "0");
        vm.setEnv("QUORUM_FLOOR", "0");
        vm.setEnv("TIMELOCK_CANCELLER", vm.toString(guardianKey));
        (timelock, governor) = governanceScript.run();
        assertEq(timelock.getMinDelay(), 180);
        assertEq(governor.votingPeriod(), 240);
        assertTrue(timelock.hasRole(timelock.CANCELLER_ROLE(), guardianKey));
    }
}
