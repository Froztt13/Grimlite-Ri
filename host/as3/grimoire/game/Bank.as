package grimoire.game
{
	import grimoire.Root;

	/**
	 * Bank lookup, transfer, swap, and UI toggle.
	 */
	public class Bank
	{
		public static function GetItemByName(name:String):Object
		{
			try
			{
				if (Root.Game.world.bankinfo != null && Root.Game.world.bankinfo.items != null)
				{
					var lower:String = name.toLowerCase();
					for each (var item:Object in Root.Game.world.bankinfo.items)
					{
						if (item != null && String(item.sName || "").toLowerCase() == lower)
						{
							return item;
						}
					}
				}
			}
			catch (e:Error) {}
			return null;
		}

		public static function HasItem(name:String, qty:int = 1):Boolean
		{
			var item:Object = GetItemByName(name);
			return item != null && (qty <= 0 || int(item.iQty || 1) >= qty);
		}

		public static function TransferToBank(itemName:String):void
		{
			try
			{
				var item:Object = Inventory.GetItemByName(itemName);
				if (item != null)
				{
					Root.Game.world.sendBankFromInvRequest(item);
				}
			}
			catch (e:Error) {}
		}

		public static function TransferToInventory(itemName:String):void
		{
			try
			{
				var item:Object = GetItemByName(itemName);
				if (item != null)
				{
					Root.Game.world.sendBankToInvRequest(item);
				}
			}
			catch (e:Error) {}
		}

		public static function BankSwap(invItemName:String, bankItemName:String):void
		{
			try
			{
				var invItem:Object = Inventory.GetItemByName(invItemName);
				var bankItem:Object = GetItemByName(bankItemName);
				if (invItem != null && bankItem != null)
				{
					Root.Game.world.sendBankSwapInvRequest(bankItem, invItem);
				}
			}
			catch (e:Error) {}
		}

		public static function Show():void
		{
			try
			{
				Root.Game.world.toggleBank();
			}
			catch (e:Error) {}
		}

		public static function LoadBankItems():void
		{
			try
			{
				Root.Game.sfc.sendXtMessage("zm", "loadBank", ["All"], "str", Root.Game.world.curRoom);
			}
			catch (e:Error) {}
		}
	}
}
