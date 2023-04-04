use super::*;
pub struct ExtBuilderPendulum {
	balances: Vec<(AccountId, PendulumCurrencyId, Balance)>,
	parachain_id: u32,
}
impl Default for ExtBuilderPendulum {
	fn default() -> Self {
		Self { balances: vec![], parachain_id: 2094 }
	}
}

impl ExtBuilderPendulum {
	pub fn balances(mut self, balances: Vec<(AccountId, PendulumCurrencyId, Balance)>) -> Self {
		self.balances = balances;
		self
	}
	pub fn parachain_id(mut self, parachain_id: u32) -> Self {
		self.parachain_id = parachain_id;
		self
	}
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![
				(AccountId::from(ALICE), INITIAL_BALANCE),
				(AccountId::from(BOB), INITIAL_BALANCE),
			],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		orml_tokens::GenesisConfig::<Runtime> {
			balances: vec![(AccountId::from(BOB), PendulumCurrencyId::XCM(0), dot(100))],
		}
		.assimilate_storage(&mut t)
		.unwrap();
		<parachain_info::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
			&parachain_info::GenesisConfig { parachain_id: self.parachain_id.into() },
			&mut t,
		)
		.unwrap();
		<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
			&pallet_xcm::GenesisConfig { safe_xcm_version: Some(2) },
			&mut t,
		)
		.unwrap();
		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}

pub struct ExtStatemintBuilder {
	balances: Vec<(AccountId, u128, Balance)>,
	parachain_id: u32,
}

impl Default for ExtStatemintBuilder {
	fn default() -> Self {
		Self { balances: vec![], parachain_id: 1000 }
	}
}

impl ExtStatemintBuilder {
	pub fn balances(mut self, balances: Vec<(AccountId, u128, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	#[allow(dead_code)]
	pub fn parachain_id(mut self, parachain_id: u32) -> Self {
		self.parachain_id = parachain_id;
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		use statemint_runtime::Runtime as StatemintRuntime;

		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<StatemintRuntime>()
			.unwrap();

		pallet_balances::GenesisConfig::<StatemintRuntime> { balances: vec![] }
			.assimilate_storage(&mut t)
			.unwrap();

		<parachain_info::GenesisConfig as GenesisBuild<StatemintRuntime>>::assimilate_storage(
			&parachain_info::GenesisConfig { parachain_id: self.parachain_id.into() },
			&mut t,
		)
		.unwrap();

		<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
			&pallet_xcm::GenesisConfig { safe_xcm_version: Some(2) },
			&mut t,
		)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
