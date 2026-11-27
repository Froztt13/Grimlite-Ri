package grimoire.game
{
	import grimoire.Root;

	/**
	 * Core player combat, skill, movement, and aura actions (Grimlite-Li behavior parity).
	 */
	public class Player
	{
		// ---- State ----

		public static function IsLoggedIn():String
		{
			return Root.Game != null && Root.Game.sfc != null && Root.Game.sfc.isConnected == true
				? Root.TrueString : Root.FalseString;
		}

		public static function HasTarget():String
		{
			return Root.Game != null && Root.Game.world != null && Root.Game.world.myAvatar != null &&
				Root.Game.world.myAvatar.target != null &&
				Root.Game.world.myAvatar.target.dataLeaf != null &&
				int(Root.Game.world.myAvatar.target.dataLeaf.intHP) > 0
				? Root.TrueString : Root.FalseString;
		}

		public static function GetTargetHealth():int
		{
			try
			{
				return int(Root.Game.world.myAvatar.target.dataLeaf.intHP);
			}
			catch (e:Error) {}
			return 0;
		}

		public static function Position():String
		{
			try
			{
				return JSON.stringify([
					Math.round(Root.Game.world.myAvatar.pMC.x),
					Math.round(Root.Game.world.myAvatar.pMC.y)
				]);
			}
			catch (e:Error) {}
			return "[0,0]";
		}

		// ---- Skills (Haste + OldCD + GCD parity with Grimlite-Li) ----

		public static function AllSkillsAvailable():int
		{
			try
			{
				var active:Array = Root.Game.world.actions.active;
				return Math.max(
					Math.max(isSkillReady(active[1]), isSkillReady(active[2])),
					Math.max(isSkillReady(active[3]), isSkillReady(active[4]))
				);
			}
			catch (e:Error) {}
			return 0;
		}

		public static function SkillAvailable(skillIndex:String):int
		{
			try
			{
				return isSkillReady(Root.Game.world.actions.active[parseInt(skillIndex)]);
			}
			catch (e:Error) {}
			return 0;
		}

		public static function ForceUseSkill(index:String):void
		{
			try
			{
				var skill:Object = Root.Game.world.actions.active[parseInt(index)];
				if (skill != null && isSkillReady(skill) == 0 && skill.isOK && !skill.skillLock)
				{
					Root.Game.world.testAction(skill);
				}
			}
			catch (e:Error) {}
		}

		public static function UseSkill(index:String):void
		{
			try
			{
				var skill:Object = Root.Game.world.actions.active[parseInt(index)];
				if (skill == null)
				{
					return;
				}
				if (skill.tgt == "s" || skill.tgt == "f")
				{
					ForceUseSkill(index);
					return;
				}
				if (Root.Game.world.myAvatar.target == Root.Game.world.myAvatar)
				{
					Root.Game.world.myAvatar.target = null;
					return;
				}
				if (Root.Game.world.myAvatar.target != null &&
					Root.Game.world.myAvatar.target.dataLeaf != null &&
					int(Root.Game.world.myAvatar.target.dataLeaf.intHP) > 0)
				{
					Root.Game.world.approachTarget();
					ForceUseSkill(index);
				}
			}
			catch (e:Error) {}
		}

		private static function isSkillReady(skill:*):int
		{
			if (skill == null)
			{
				return 0;
			}
			var now:Number = new Date().getTime();
			var tha:Number = 0;
			if (Root.Game.world.myAvatar != null &&
				Root.Game.world.myAvatar.dataLeaf != null &&
				Root.Game.world.myAvatar.dataLeaf.sta != null)
			{
				tha = Number(Root.Game.world.myAvatar.dataLeaf.sta.$tha || 0);
			}
			var hasteMul:Number = 1 - Math.min(Math.max(tha, -1), 0.5);
			var effectiveCd:Number;
			if (skill.OldCD != null)
			{
				effectiveCd = Math.round(Number(skill.OldCD) * hasteMul);
				delete skill.OldCD;
			}
			else
			{
				effectiveCd = Math.round(Number(skill.cd) * hasteMul);
			}
			var gcdRemain:Number = Number(Root.Game.world.GCD) - (now - Number(Root.Game.world.GCDTS));
			if (gcdRemain < 0 || isNaN(gcdRemain))
			{
				gcdRemain = 0;
			}
			var cdRemain:Number = effectiveCd - (now - Number(skill.ts));
			if (cdRemain < 0 || isNaN(cdRemain))
			{
				cdRemain = 0;
			}
			return int(Math.max(gcdRemain, cdRemain));
		}

		// ---- Combat Targeting ----

		public static function AttackMonster(monsterName:String):void
		{
			targetMonster(World.GetMonsterByName(monsterName));
		}

		public static function AttackMonsterByMonMapId(monMapID:String):void
		{
			targetMonster(World.GetMonsterByMonMapId(monMapID));
		}

		private static function targetMonster(monster:*):void
		{
			if (monster == null)
			{
				return;
			}
			try
			{
				Root.Game.world.setTarget(monster);
				Root.Game.world.approachTarget();
			}
			catch (e:Error) {}
		}

		public static function SetTargetPlayer(username:String):void
		{
			try
			{
				var avatar:* = Root.Game.world.getAvatarByUserName(username);
				if (avatar != null)
				{
					Root.Game.world.setTarget(avatar);
				}
			}
			catch (e:Error) {}
		}

		public static function CancelAutoAttack():void
		{
			try
			{
				Root.Game.world.cancelAutoAttack();
			}
			catch (e:Error) {}
		}

		public static function CancelTarget():void
		{
			try
			{
				Root.Game.world.cancelTarget();
				Root.Game.world.cancelTarget();
			}
			catch (e:Error) {}
		}

		public static function CancelTargetSelf():void
		{
			try
			{
				if (Root.Game.world.myAvatar.target == Root.Game.world.myAvatar)
				{
					Root.Game.world.cancelTarget();
				}
			}
			catch (e:Error) {}
		}

		// ---- Movement & Actions ----

		public static function Join(map:String, cell:String = "Enter", pad:String = "Spawn"):void
		{
			try
			{
				Root.Game.world.gotoTown(map, cell, pad);
			}
			catch (e:Error) {}
		}

		public static function Jump(cell:String, pad:String = "Spawn"):void
		{
			try
			{
				Root.Game.world.moveToCell(cell, pad);
			}
			catch (e:Error) {}
		}

		public static function WalkToPoint(strX:String, strY:String):void
		{
			try
			{
				var x:int = parseInt(strX);
				var y:int = parseInt(strY);
				var pMC:* = Root.Game.world.myAvatar.pMC;
				var spd:int = int(Root.Game.world.WALKSPEED);
				pMC.walkTo(x, y, spd);
				Root.Game.world.moveRequest({mc:pMC, tx:x, ty:y, sp:spd});
			}
			catch (e:Error) {}
		}

		public static function SetWalkSpeed(speed:String):void
		{
			try
			{
				var spd:int = parseInt(speed);
				if (spd > 0)
				{
					Root.Game.world.WALKSPEED = spd;
				}
			}
			catch (e:Error) {}
		}

		public static function Rest():void
		{
			try
			{
				Root.Game.world.rest();
			}
			catch (e:Error) {}
		}

		public static function GoTo(username:String):void
		{
			try
			{
				Root.Game.world["goto"](username);
			}
			catch (e:Error) {}
		}

		public static function Equip(itemId:String):void
		{
			try
			{
				Root.Game.world.sendEquipItemRequest({ItemID:parseInt(itemId)});
			}
			catch (e:Error) {}
		}

		public static function EquipPotion(itemId:String, desc:String, file:String, name:String):void
		{
			try
			{
				Root.Game.world.equipUseableItem({ItemID:parseInt(itemId), sDesc:desc, sFile:file, sName:name});
			}
			catch (e:Error) {}
		}

		public static function GetMapItem(itemId:String):void
		{
			try
			{
				Root.Game.world.getMapItem(parseInt(itemId));
			}
			catch (e:Error) {}
		}

		public static function Logout():void
		{
			try
			{
				Root.Game.logout();
			}
			catch (e:Error) {}
		}

		// ---- Auras ----

		public static function GetAurasValue(self:String, auraName:String):int
		{
			var aura:* = findAura(self == "True" || self == "true", auraName);
			if (aura == null)
			{
				return 0;
			}
			var numericVal:Number = parseFloat(aura.val);
			return (!isNaN(numericVal) && numericVal > 0) ? Math.ceil(numericVal) : 1;
		}

		public static function GetAurasDuration(self:String, auraName:String):String
		{
			var isSelf:Boolean = (self == "True" || self == "true");
			if (!isSelf && HasTarget() != Root.TrueString)
			{
				return "Invalid Target";
			}
			var aura:* = findAura(isSelf, auraName);
			if (aura == null)
			{
				return "Aura Not Found";
			}
			var dur:int = int(aura.dur) * 1000;
			var elapsed:int = int((new Date().getTime() - 1000) - Number(aura.ts));
			return dur + "/" + elapsed;
		}

		private static function findAura(isSelf:Boolean, auraName:String):*
		{
			try
			{
				if (!isSelf && HasTarget() != Root.TrueString)
				{
					return null;
				}
				var list:* = isSelf
					? Root.Game.world.myAvatar.dataLeaf.auras
					: Root.Game.world.myAvatar.target.dataLeaf.auras;
				var wanted:String = auraName.toLowerCase().replace(" ", "");
				for each (var aura:* in list)
				{
					if (aura != null && String(aura.nam).toLowerCase().replace(" ", "") == wanted)
					{
						return aura;
					}
				}
			}
			catch (e:Error) {}
			return null;
		}
	}
}
