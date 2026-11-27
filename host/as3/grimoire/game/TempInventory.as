package grimoire.game
{
	import grimoire.Root;

	/**
	 * Player temporary inventory item lookup.
	 */
	public class TempInventory
	{
		public static function GetTempItemByName(name:String):Object
		{
			try
			{
				var lower:String = name.toLowerCase();
				for each (var item:Object in Root.Game.world.myAvatar.tempitems)
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

		public static function ItemIsInTemp(itemName:String, itemQty:String):String
		{
			var item:Object = GetTempItemByName(itemName);
			if (item == null)
			{
				return Root.FalseString;
			}
			return (itemQty == "*" || int(item.iQty) >= parseInt(itemQty))
				? Root.TrueString : Root.FalseString;
		}
	}
}
