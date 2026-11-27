package grimoire.game
{
	import grimoire.Root;

	/**
	 * Auto-relogin helpers for detecting kicks, logging in, and connecting to a server.
	 */
	public class AutoRelogin
	{
		public static function IsTemporarilyKicked():String
		{
			try
			{
				return (Root.Game.mcLogin != null &&
					Root.Game.mcLogin.btnLogin != null &&
					Root.Game.mcLogin.btnLogin.visible == false) ? Root.TrueString : Root.FalseString;
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		public static function Login():void
		{
			FixLogin(Root.Username, Root.Password);
		}

		public static function FixLogin(username:String, password:String):void
		{
			try
			{
				if (Root.Game.charCount() > 0)
				{
					Root.Game.removeAllChildren();
					Root.Game.gotoAndPlay("Login");
				}
				Root.Username = username;
				Root.Password = password;
				Root.Game.login(username, password);
			}
			catch (e:Error) {}
		}

		public static function AreServersLoaded():String
		{
			try
			{
				if (Root.Game.serialCmd != null && Root.Game.serialCmd.servers != null)
				{
					return (Root.Game.serialCmd.servers.length > 0) ? Root.TrueString : Root.FalseString;
				}
			}
			catch (e:Error) {}
			return Root.FalseString;
		}

		public static function Connect(name:String):void
		{
			try
			{
				for each (var server:Object in Root.Game.serialCmd.servers)
				{
					if (server.sName == name)
					{
						server.iMax = 5000;
						Root.Game.objServerInfo = server;
						Root.Game.chatF.iChat = server.iChat;
						break;
					}
				}
				Root.Game.connectTo(Root.Game.objServerInfo.sIP, Root.Game.objServerInfo.iPort);
			}
			catch (e:Error) {}
		}
	}
}
