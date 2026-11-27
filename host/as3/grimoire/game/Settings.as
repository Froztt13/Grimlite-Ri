package grimoire.game
{
	import grimoire.Root;

	/**
	 * Runtime client toggles (Infinite Range, Provoke, Enemy Magnet, Lag Killer, Skip Cutscenes, Disable Animations).
	 */
	public class Settings
	{
		public static function SetInfiniteRange(enabled:Boolean = true):void
		{
			try
			{
				var range:int = enabled ? 20000 : 300;
				var active:Array = Root.Game.world.actions.active;
				if (active != null)
				{
					for (var i:int = 0; i < active.length; i++)
					{
						if (active[i] != null)
						{
							active[i].range = range;
						}
					}
				}
			}
			catch (e:Error) {}
		}

		public static function SetProvokeMonsters():void
		{
			try
			{
				Root.Game.world.aggroAllMon();
			}
			catch (e:Error) {}
		}

		public static function SetEnemyMagnet():void
		{
			try
			{
				var target:* = Root.Game.world.myAvatar.target;
				var me:* = Root.Game.world.myAvatar.pMC;
				if (target != null && target.pMC != null && me != null)
				{
					target.pMC.x = me.x;
					target.pMC.y = me.y;
				}
			}
			catch (e:Error) {}
		}

		public static function SetLagKiller(state:String):void
		{
			try
			{
				Root.Game.world.visible = (state == "False" || state == "false");
			}
			catch (e:Error) {}
		}

		public static function SetSkipCutscenes():void
		{
			try
			{
				if (Root.Game.mcExtSWF != null && Root.Game.mcExtSWF.numChildren > 0)
				{
					while (Root.Game.mcExtSWF.numChildren > 0)
					{
						Root.Game.mcExtSWF.removeChildAt(0);
					}
					Root.Game.world.visible = true;
					if (Root.Game.showInterface is Function)
					{
						Root.Game.showInterface();
					}
				}
			}
			catch (e:Error) {}
		}

		public static function DisableAnimationsFrame():void
		{
			try
			{
				var avatars:Object = Root.Game.world.avatars;
				for (var key:String in avatars)
				{
					var av:* = avatars[key];
					if (av != null && av.pMC != null && av.pMC.mcChar != null)
					{
						av.pMC.mcChar.gotoAndStop("Idle");
					}
				}
			}
			catch (e:Error) {}
		}
	}
}
