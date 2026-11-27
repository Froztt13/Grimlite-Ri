package grimoire
{
	import flash.external.ExternalInterface;
	import grimoire.game.*;

	/**
	 * Registers canonical Grimlite-Li ExternalInterface callbacks for `grimoire.game.*` and `Caller`.
	 */
	public class Externalizer
	{
		public static function registerAll():void
		{
			if (!ExternalInterface.available)
			{
				return;
			}

			// Player & Combat
			bind("IsLoggedIn", Player.IsLoggedIn);
			bind("HasTarget", Player.HasTarget);
			bind("GetTargetHealth", Player.GetTargetHealth);
			bind("AllSkillsAvailable", Player.AllSkillsAvailable);
			bind("SkillAvailable", Player.SkillAvailable);
			bind("Position", Player.Position);
			bind("WalkToPoint", Player.WalkToPoint);
			bind("CancelAutoAttack", Player.CancelAutoAttack);
			bind("CancelTarget", Player.CancelTarget);
			bind("CancelTargetSelf", Player.CancelTargetSelf);
			bind("AttackMonster", Player.AttackMonster);
			bind("AttackMonsterByMonMapId", Player.AttackMonsterByMonMapId);
			bind("Jump", Player.Jump);
			bind("Rest", Player.Rest);
			bind("Join", Player.Join);
			bind("Equip", Player.Equip);
			bind("EquipPotion", Player.EquipPotion);
			bind("GoTo", Player.GoTo);
			bind("UseSkill", Player.UseSkill);
			bind("ForceUseSkill", Player.ForceUseSkill);
			bind("GetMapItem", Player.GetMapItem);
			bind("Logout", Player.Logout);
			bind("SetTargetPlayer", Player.SetTargetPlayer);
			bind("SetWalkSpeed", Player.SetWalkSpeed);
			bind("GetAurasValue", Player.GetAurasValue);
			bind("AuraDuration", Player.GetAurasDuration);

			// World
			bind("MapLoadComplete", World.MapLoadComplete);
			bind("PlayersInMap", World.PlayersInMap);
			bind("IsActionAvailable", World.IsActionAvailable);
			bind("GetMonstersInCell", World.GetMonstersInCell);
			bind("GetVisibleMonstersInCell", World.GetVisibleMonstersInCell);
			bind("SetSpawnPoint", World.SetSpawnPoint);
			bind("IsMonsterAvailable", World.IsMonsterAvailable);
			bind("IsMonsterAvailableByMonMapID", World.IsMonsterAvailableByMonMapID);
			bind("GetSkillName", World.GetSkillName);
			bind("GetCells", World.GetCells);
			bind("RoomId", World.RoomId);
			bind("RoomNumber", World.RoomNumber);
			bind("RejectDrop", World.RejectDrop);

			// Quests
			bind("IsInProgress", Quests.IsInProgress);
			bind("Complete", Quests.Complete);
			bind("Accept", Quests.Accept);
			bind("LoadQuest", Quests.Load);
			bind("CanComplete", Quests.CanComplete);
			bind("IsAvailable", Quests.IsAvailable);

			// Shops, Bank & TempInventory
			bind("LoadShop", Shops.Load);
			bind("SellItem", Shops.SellItem);
			bind("IsShopLoaded", Shops.IsShopLoaded);
			bind("BuyItem", Shops.BuyItem);
			bind("BuyItemQty", Shops.BuyItemQty);
			bind("BuyItemQtyById", Shops.BuyItemQtyById);
			bind("TransferToBank", Bank.TransferToBank);
			bind("TransferToInventory", Bank.TransferToInventory);
			bind("BankSwap", Bank.BankSwap);
			bind("ShowBank", Bank.Show);
			bind("LoadBankItems", Bank.LoadBankItems);
			bind("ItemIsInTemp", TempInventory.ItemIsInTemp);

			// AutoRelogin & Settings
			bind("IsTemporarilyKicked", AutoRelogin.IsTemporarilyKicked);
			bind("Login", AutoRelogin.Login);
			bind("FixLogin", AutoRelogin.FixLogin);
			bind("AreServersLoaded", AutoRelogin.AreServersLoaded);
			bind("Connect", AutoRelogin.Connect);
			bind("SetInfiniteRange", Settings.SetInfiniteRange);
			bind("SetProvokeMonsters", Settings.SetProvokeMonsters);
			bind("SetEnemyMagnet", Settings.SetEnemyMagnet);
			bind("SetLagKiller", Settings.SetLagKiller);
			bind("SetSkipCutscenes", Settings.SetSkipCutscenes);
			bind("SetFPS", Root.setFPS);
			bind("SendMessage", Root.sendMessage);

			// Generic Reflection Bridge
			bind("getGameObject", Caller.getGameObject);
			bind("setGameObject", Caller.setGameObject);
			bind("callGameFunction", Caller.callGameFunction);
			bind("callGameFunction0", Caller.callGameFunction0);
			bind("isNull", Caller.isNull);
		}

		private static function bind(name:String, func:Function):void
		{
			ExternalInterface.addCallback(name, func);
		}
	}
}
