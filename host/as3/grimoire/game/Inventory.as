package grimoire.game
{
	import grimoire.Root;

	/**
	 * Player inventory item lookup and equip-status helpers.
	 */
	public class Inventory
	{
		public static function GetItemByName(name:String):Object
		{
			try
			{
				var lower:String = name.toLowerCase();
				for each (var item:Object in Root.Game.world.myAvatar.items)
				{
					if (item != null && String(item.sName || "").toLowerCase() == lower)
					{
						return item;
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

		public static function IsEquipped(name:String):Boolean
		{
			var item:Object = GetItemByName(name);
			return item != null && int(item.bEquip) == 1;
		}
	}
}
