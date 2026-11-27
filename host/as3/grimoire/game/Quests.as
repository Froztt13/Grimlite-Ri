package grimoire.game
{
	import grimoire.Root;

	/**
	 * Quest loading, acceptance, validation, and turn-in (Grimlite-Li behavior parity).
	 */
	public class Quests
	{
		public static function Load(id:String):void
		{
			try
			{
				Root.Game.world.showQuests([id], "q");
			}
			catch (e:Error) {}
		}

		public static function Accept(id:String):void
		{
			try
			{
				Root.Game.world.acceptQuest(parseInt(id));
			}
			catch (e:Error) {}
		}

		public static function Complete(id:String, qty:int = 1, itemID:String = "-1", special:String = "False"):void
		{
			try
			{
				var qid:int = parseInt(id);
				var turnIns:int = qty;
				if (turnIns < 0 && Root.Game.world.maximumQuestTurnIns is Function)
				{
					turnIns = int(Root.Game.world.maximumQuestTurnIns(qid));
				}
				if (turnIns < 1)
				{
					turnIns = 1;
				}
				Root.Game.world.tryQuestComplete(qid, parseInt(itemID), special == "True", turnIns);
			}
			catch (e:Error) {}
		}

		public static function IsLoaded(id:int):Boolean
		{
			try
			{
				return Root.Game.world.questTree != null && Root.Game.world.questTree[id] != null;
			}
			catch (e:Error) {}
			return false;
		}

		public static function IsCompletedOnce(id:int):Boolean
		{
			try
			{
				var q:* = Root.Game.world.questTree[id];
				if (q != null && int(q.bOnce) == 1 && int(q.iSlot) > 0)
				{
					return int(Root.Game.world.getQuestValue(int(q.iSlot))) >= int(q.iValue);
				}
			}
			catch (e:Error) {}
			return false;
		}

		public static function IsInProgress(id:String):String
		{
			try
			{
				return Root.Game.world.isQuestInProgress(parseInt(id)) ? Root.TrueString : Root.FalseString;
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		public static function IsAvailable(id:String):String
		{
			try
			{
				return getValidationError(parseInt(id)) == "" ? Root.TrueString : Root.FalseString;
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		public static function CanComplete(id:String):String
		{
			try
			{
				var qid:int = parseInt(id);
				return Root.Game.world.canTurnInQuest(qid) && getValidationError(qid) == ""
					? Root.TrueString : Root.FalseString;
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		private static function hasRequiredItems(q:Object):Boolean
		{
			if (q.reqd != null && q.reqd.length > 0)
			{
				for each (var req:* in q.reqd)
				{
					var owned:* = Root.Game.world.invTree[int(req.ItemID)];
					if (owned == null || int(owned.iQty) < int(req.iQty))
					{
						return false;
					}
				}
			}
			return true;
		}

		private static function getValidationError(qid:int):String
		{
			var q:Object = Root.Game.world.questTree[qid];
			if (q == null)
			{
				return "Quest not loaded.";
			}
			if (q.sField != null && Root.Game.world.getAchievement(q.sField, q.iIndex) != 0)
			{
				return q.sField == "im0" ? "Monthly limit reached." : "Daily limit reached.";
			}
			if (q.bUpg == 1 && !Root.Game.world.myAvatar.isUpgraded())
			{
				return "Upgrade required.";
			}
			if (q.iSlot >= 0 && Root.Game.world.getQuestValue(q.iSlot) < q.iValue - 1)
			{
				return "Quest locked.";
			}
			if (q.iLvl > Root.Game.world.myAvatar.objData.intLevel)
			{
				return "Level requirement not met.";
			}
			if (q.iClass > 0 && Root.Game.world.myAvatar.getCPByID(q.iClass) < q.iReqCP)
			{
				return "Class rank requirement not met.";
			}
			if (q.FactionID > 1 && Root.Game.world.myAvatar.getRep(q.FactionID) < q.iReqRep)
			{
				return "Reputation requirement not met.";
			}
			if (q.reqd != null && !hasRequiredItems(q))
			{
				return "Missing required items.";
			}
			return "";
		}
	}
}
