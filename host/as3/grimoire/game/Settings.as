package grimoire.game
{
	import flash.display.DisplayObject;
	import flash.display.DisplayObjectContainer;
	import flash.display.Loader;
	import flash.display.Sprite;
	import flash.events.Event;
	import flash.events.IOErrorEvent;
	import flash.utils.ByteArray;
	import flash.utils.Dictionary;
	import grimoire.Root;

	/**
	 * Runtime client toggles (Infinite Range, Provoke, Enemy Magnet, Lag Killer + Custom Wallpaper, Skip Cutscenes, Hide Players, Disable FX/Animations).
	 */
	public class Settings
	{
		private static var lagEnabled:Boolean = false;
		private static var lagBg:Sprite = null;
		private static var lagLoader:Loader = null;
		private static var lagHasImage:Boolean = false;
		private static var savedFX:Dictionary = new Dictionary(true);

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
				lagEnabled = !(state == "False" || state == "false");
				if (Root.Game != null && Root.Game.world != null)
				{
					Root.Game.world.visible = !lagEnabled;
				}
				syncLagBg();
			}
			catch (e:Error) {}
		}

		/**
		 * Loads a custom PNG/JPG wallpaper (base64-encoded) shown behind AQW's UI whenever Lag Killer is ON.
		 * Pass an empty string to clear the wallpaper and return to plain black.
		 */
		public static function SetLagKillerImage(base64Data:String):void
		{
			try
			{
				if (lagBg == null)
				{
					lagBg = new Sprite();
					lagBg.mouseEnabled = false;
					lagBg.mouseChildren = false;
				}
				while (lagBg.numChildren > 0)
				{
					lagBg.removeChildAt(0);
				}
				lagLoader = null;
				lagHasImage = false;

				if (base64Data == null || base64Data.length == 0)
				{
					syncLagBg();
					return;
				}

				var bytes:ByteArray = decodeBase64(base64Data);
				if (bytes.length == 0)
				{
					syncLagBg();
					return;
				}

				lagLoader = new Loader();
				lagLoader.contentLoaderInfo.addEventListener(Event.COMPLETE, onLagImageLoaded);
				lagLoader.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, onLagImageError);
				lagLoader.loadBytes(bytes);
			}
			catch (e:Error) {}
		}

		private static function onLagImageLoaded(event:Event):void
		{
			try
			{
				if (lagBg == null || lagLoader == null)
				{
					return;
				}
				while (lagBg.numChildren > 0)
				{
					lagBg.removeChildAt(0);
				}

				var content:DisplayObject = lagLoader.content;
				var stageW:Number = 960;
				var stageH:Number = 550;
				if (content != null && content.width > 0 && content.height > 0)
				{
					var scaleX:Number = stageW / content.width;
					var scaleY:Number = stageH / content.height;
					var scale:Number = scaleX > scaleY ? scaleX : scaleY;
					content.scaleX = scale;
					content.scaleY = scale;
					content.x = (stageW - content.width) * 0.5;
					content.y = (stageH - content.height) * 0.5;
				}
				lagBg.addChild(lagLoader);
				lagHasImage = true;
				syncLagBg();
			}
			catch (e:Error) {}
		}

		private static function onLagImageError(event:IOErrorEvent):void
		{
			lagHasImage = false;
			syncLagBg();
		}

		private static function syncLagBg():void
		{
			try
			{
				if (lagBg == null)
				{
					return;
				}
				if (Root.Game != null && Root.Game is DisplayObjectContainer && Root.Game.world != null)
				{
					var container:DisplayObjectContainer = DisplayObjectContainer(Root.Game);
					var worldObj:DisplayObject = DisplayObject(Root.Game.world);
					if (worldObj.parent == container)
					{
						var worldIdx:int = container.getChildIndex(worldObj);
						if (lagBg.parent != container)
						{
							container.addChildAt(lagBg, worldIdx);
						}
						else
						{
							var curIdx:int = container.getChildIndex(lagBg);
							if (curIdx > worldIdx)
							{
								container.setChildIndex(lagBg, worldIdx);
							}
						}
					}
				}
				lagBg.visible = lagEnabled && lagHasImage;
			}
			catch (e:Error) {}
		}

		private static function decodeBase64(data:String):ByteArray
		{
			var out:ByteArray = new ByteArray();
			var alphabet:String = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
			var buf:int = 0;
			var bits:int = 0;
			var len:int = data.length;
			for (var i:int = 0; i < len; i++)
			{
				var ch:String = data.charAt(i);
				if (ch == "=")
				{
					break;
				}
				var val:int = alphabet.indexOf(ch);
				if (val < 0)
				{
					continue;
				}
				buf = (buf << 6) | val;
				bits += 6;
				if (bits >= 8)
				{
					bits -= 8;
					out.writeByte((buf >> bits) & 0xFF);
				}
			}
			out.position = 0;
			return out;
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

		public static function SetHidePlayers(enabled:Boolean):void
		{
			try
			{
				if (Root.Game == null || Root.Game.world == null)
				{
					return;
				}
				var myAv:* = Root.Game.world.myAvatar;
				var avatars:Object = Root.Game.world.avatars;
				for (var key:String in avatars)
				{
					var av:* = avatars[key];
					if (av != null && av !== myAv && av.pMC != null)
					{
						av.pMC.visible = !enabled;
					}
				}
			}
			catch (e:Error) {}
		}

		public static function SetDisableAnimations(enabled:Boolean):void
		{
			if (!enabled)
			{
				try
				{
					for (var mc:* in savedFX)
					{
						if (mc != null && mc.spFX == null)
						{
							mc.spFX = savedFX[mc];
						}
					}
					savedFX = new Dictionary(true);
				}
				catch (e:Error) {}
			}
		}

		public static function DisableAnimationsFrame():void
		{
			try
			{
				var avatars:Object = Root.Game.world.avatars;
				for (var key:String in avatars)
				{
					var av:* = avatars[key];
					if (av != null && av.pMC != null)
					{
						if (av.pMC.spFX != null)
						{
							savedFX[av.pMC] = av.pMC.spFX;
							av.pMC.spFX = null;
						}
						if (av.pMC.mcChar != null)
						{
							av.pMC.mcChar.gotoAndStop("Idle");
						}
					}
				}
			}
			catch (e:Error) {}
		}
	}
}
