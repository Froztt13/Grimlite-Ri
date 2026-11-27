package grimoire.game
{
	import grimoire.Root;

	/**
	 * Shop loading, buying (single / quantity / by ID), and selling.
	 */
	public class Shops
	{
		public static function Load(id:String):void
		{
			try
			{
				Root.Game.world.sendLoadShopRequest(parseInt(id));
			}
			catch (e:Error) {}
		}

		public static function IsShopLoaded(shopId:int = 0):String
		{
			try
			{
				var info:* = Root.Game.world.shopinfo;
				if (info != null && info.items != null && info.items.length > 0)
				{
					if (shopId <= 0 || int(info.ShopID) == shopId)
					{
						return Root.TrueString;
					}
				}
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		public static function BuyItem(name:String):void
		{
			try
			{
				var item:Object = getShopItem(name.toLowerCase());
				if (item != null)
				{
					Root.Game.world.sendBuyItemRequest(item);
				}
			}
			catch (e:Error) {}
		}

		public static function BuyItemQty(name:String, qty:int):void
		{
			buyWithQty(getShopItem(name.toLowerCase()), qty);
		}

		public static function BuyItemQtyById(qty:int, itemId:int, shopItemId:int):void
		{
			buyWithQty(getShopItemById(itemId, shopItemId), qty);
		}

		public static function SellItem(name:String):void
		{
			try
			{
				var item:Object = Inventory.GetItemByName(name);
				if (item != null)
				{
					Root.Game.world.sendSellItemRequest(item);
				}
			}
			catch (e:Error) {}
		}

		private static function buyWithQty(item:Object, qty:int):void
		{
			if (item == null)
			{
				return;
			}
			try
			{
				Root.Game.world.sendBuyItemRequestWithQuantity({
					accept: 1,
					iQty: qty,
					iSel: item
				});
			}
			catch (e:Error) {}
		}

		private static function getShopItem(lowerName:String):Object
		{
			try
			{
				var items:Array = Root.Game.world.shopinfo.items;
				for (var i:int = 0; i < items.length; i++)
				{
					var item:Object = items[i];
					if (item != null && String(item.sName || "").toLowerCase() == lowerName)
					{
						return item;
					}
				}
			}
			catch (e:Error) {}
			return null;
		}

		private static function getShopItemById(itemId:int, shopItemId:int):Object
		{
			try
			{
				var items:Array = Root.Game.world.shopinfo.items;
				for (var i:int = 0; i < items.length; i++)
				{
					var item:Object = items[i];
					if (item != null && int(item.ItemID) == itemId && int(item.ShopItemID) == shopItemId)
					{
						return item;
					}
				}
			}
			catch (e:Error) {}
			return null;
		}
	}
}
