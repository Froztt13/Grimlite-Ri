package grimoire.game
{
	import grimoire.Root;

	/**
	 * World queries, monster lookup, map loading status, and drop rejection.
	 */
	public class World
	{
		public static function MapLoadComplete():String
		{
			try
			{
				if (!Root.Game.world.mapLoadInProgress)
				{
					return Root.Game.getChildAt(Root.Game.numChildren - 1) != Root.Game.mcConnDetail
						? Root.TrueString : Root.FalseString;
				}
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		public static function IsActionAvailable(actionKey:String):String
		{
			try
			{
				var lockObj:* = Root.Game.world.lock[actionKey];
				if (lockObj == null)
				{
					return Root.TrueString;
				}
				var elapsed:Number = new Date().getTime() - Number(lockObj.ts);
				return elapsed < Number(lockObj.cd) ? Root.FalseString : Root.TrueString;
			}
			catch (e:Error) {}
			return Root.TrueString;
		}

		public static function SetSpawnPoint():void
		{
			try
			{
				Root.Game.world.setSpawnPoint(Root.Game.world.strFrame, Root.Game.world.strPad);
			}
			catch (e:Error) {}
		}

		public static function GetCells():String
		{
			var cells:Array = [];
			try
			{
				var labels:Array = null;
				if (Root.Game.world.map != null && Root.Game.world.map.currentScene != null && Root.Game.world.map.currentScene.labels != null)
				{
					labels = Root.Game.world.map.currentScene.labels;
				}
				else if (Root.Game.world.map != null && Root.Game.world.map.currentLabels != null)
				{
					labels = Root.Game.world.map.currentLabels;
				}
				if (labels != null)
				{
					for each (var cell:Object in labels)
					{
						if (cell != null && cell.name != null)
						{
							cells.push(String(cell.name));
						}
					}
				}
			}
			catch (e:Error) {}
			return JSON.stringify(cells);
		}

		public static function PlayersInMap():String
		{
			try
			{
				return JSON.stringify(Root.Game.world.areaUsers);
			}
			catch (e:Error) {}
			return "[]";
		}

		public static function GetCellPlayers(username:String):String
		{
			try
			{
				var target:String = (username != null) ? username.replace(/^\s+|\s+$/g, "").toLowerCase() : "";
				if (target.length == 0 || Root.Game.world == null || Root.Game.world.uoTree == null)
				{
					return Root.FalseString;
				}
				var myFrame:String = String(Root.Game.world.strFrame || "").toLowerCase();
				var entry:Object = Root.Game.world.uoTree[target];
				if (entry != null && entry.strFrame != null)
				{
					return String(entry.strFrame).toLowerCase() == myFrame ? Root.TrueString : Root.FalseString;
				}
				for each (var uo:Object in Root.Game.world.uoTree)
				{
					if (uo != null && uo.uoName != null && String(uo.uoName).toLowerCase() == target)
					{
						return String(uo.strFrame || "").toLowerCase() == myFrame ? Root.TrueString : Root.FalseString;
					}
				}
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		public static function GetPlayerCellPad(username:String):String
		{
			try
			{
				var target:String = (username != null) ? username.replace(/^\s+|\s+$/g, "").toLowerCase() : "";
				if (target.length > 0 && Root.Game.world != null && Root.Game.world.uoTree != null)
				{
					var entry:Object = Root.Game.world.uoTree[target];
					if (entry == null)
					{
						for each (var uo:Object in Root.Game.world.uoTree)
						{
							if (uo != null && uo.uoName != null && String(uo.uoName).toLowerCase() == target)
							{
								entry = uo;
								break;
							}
						}
					}
					if (entry != null && entry.strFrame != null && String(entry.strFrame).length > 0)
					{
						var padStr:String = (entry.strPad != null && String(entry.strPad).length > 0) ? String(entry.strPad) : "Spawn";
						return JSON.stringify({ cell: String(entry.strFrame), pad: padStr });
					}
				}
			}
			catch (e:Error) {}
			return "{}";
		}

		public static function RoomId():String
		{
			try
			{
				return String(Root.Game.world.curRoom);
			}
			catch (e:Error) {}
			return "1";
		}

		public static function RoomNumber():String
		{
			try
			{
				return String(Root.Game.world.strAreaName).split("-")[1];
			}
			catch (e:Error) {}
			return "1";
		}

		public static function GetSkillName(index:String):String
		{
			try
			{
				return "\"" + Root.Game.world.actions.active[parseInt(index)].nam + "\"";
			}
			catch (e:Error) {}
			return "\"\"";
		}

		// ---- Monsters ----

		public static function IsMonsterAvailable(name:String):String
		{
			return GetMonsterByName(name) != null ? Root.TrueString : Root.FalseString;
		}

		public static function IsMonsterAvailableByMonMapID(monMapID:String):String
		{
			return GetMonsterByMonMapId(monMapID) != null ? Root.TrueString : Root.FalseString;
		}

		public static function GetMonsterByName(name:String):Object
		{
			try
			{
				var raw:String = (name != null) ? name.replace(/^\s+|\s+$/g, "") : "*";
				var lower:String = raw.toLowerCase();
				if (lower.indexOf("id'") == 0 || lower.indexOf("id.") == 0 || lower.indexOf("id:") == 0 || lower.indexOf("id-") == 0)
				{
					return GetMonsterByMonMapId(lower.substr(3));
				}
				var list:Array = Root.Game.world.getMonstersByCell(Root.Game.world.strFrame);
				for each (var mon:Object in list)
				{
					if (mon != null && mon.pMC != null && mon.dataLeaf != null &&
						int(mon.dataLeaf.intState) > 0 && int(mon.dataLeaf.intHP) > 0)
					{
						var monName:String = "";
						if (mon.pMC.pname != null && mon.pMC.pname.ti != null && mon.pMC.pname.ti.text != null)
						{
							monName = String(mon.pMC.pname.ti.text).toLowerCase();
						}
						else if (mon.objData != null && mon.objData.strMonName != null)
						{
							monName = String(mon.objData.strMonName).toLowerCase();
						}
						if (lower == "*" || lower.length == 0 || monName.indexOf(lower) > -1)
						{
							return mon;
						}
					}
				}
			}
			catch (e:Error) {}
			return null;
		}

		public static function GetMonsterByMonMapId(monId:String):Object
		{
			try
			{
				var list:Array = Root.Game.world.getMonstersByCell(Root.Game.world.strFrame);
				for each (var mon:Object in list)
				{
					if (mon != null && mon.pMC != null && mon.dataLeaf != null &&
						int(mon.dataLeaf.intState) > 0 && int(mon.dataLeaf.intHP) > 0)
					{
						var mid:String = String(mon.dataLeaf.MonMapID);
						if (mid == monId || mid.indexOf(monId) > -1 || monId == "*")
						{
							return mon;
						}
					}
				}
			}
			catch (e:Error) {}
			return null;
		}

		public static function GetMonstersInCell(cell:String):String
		{
			return serializeMonsters(cell, false);
		}

		public static function GetVisibleMonstersInCell():String
		{
			try
			{
				return serializeMonsters(String(Root.Game.world.strFrame), true);
			}
			catch (e:Error) {}
			return "[]";
		}

		private static function serializeMonsters(cell:String, onlyVisible:Boolean):String
		{
			var ret:Array = [];
			try
			{
				var mons:Array = Root.Game.world.getMonstersByCell(cell);
				for each (var m:Object in mons)
				{
					if (m == null || m.objData == null || m.dataLeaf == null)
					{
						continue;
					}
					if (onlyVisible && (m.pMC == null || !m.pMC.visible || int(m.dataLeaf.intState) <= 0))
					{
						continue;
					}
					ret.push({
						sRace: m.objData.sRace,
						strMonName: m.objData.strMonName,
						MonID: m.dataLeaf.MonID,
						MonMapID: m.dataLeaf.MonMapID,
						iLvl: m.dataLeaf.iLvl,
						intState: m.dataLeaf.intState,
						intHP: m.dataLeaf.intHP,
						intHPMax: m.dataLeaf.intHPMax
					});
				}
			}
			catch (e:Error) {}
			return JSON.stringify(ret);
		}

		// ---- Drops ----

		public static function RejectDrop(itemName:String, itemId:String):void
		{
			try
			{
				if (Root.Game.litePreference != null && Root.Game.litePreference.data.bCustomDrops)
				{
					Root.Game.cDropsUI.onBtNo(Root.Game.world.invTree[itemId]);
				}
				else
				{
					var lower:String = itemName.toLowerCase();
					for (var i:int = Root.Game.ui.dropStack.numChildren - 1; i >= 0; i--)
					{
						var child:* = Root.Game.ui.dropStack.getChildAt(i);
						if (child != null && child.cnt != null && String(child.cnt.strName.text).toLowerCase().indexOf(lower) == 0)
						{
							Root.Game.ui.dropStack.removeChild(child);
						}
					}
				}
			}
			catch (e:Error) {}
		}
	}
}
