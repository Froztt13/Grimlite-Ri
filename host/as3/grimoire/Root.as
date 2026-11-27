package grimoire
{
	import flash.display.Stage;
	import flash.system.ApplicationDomain;

	/**
	 * Shared static game reference for `grimoire.game.*`, populated by `grimlite.host.Main`.
	 */
	public class Root
	{
		public static var GameDomain:ApplicationDomain;
		public static var Game:*;
		public static var StageRef:Stage;
		public static const TrueString:String = "\"True\"";
		public static const FalseString:String = "\"False\"";
		public static var Username:String = "";
		public static var Password:String = "";

		public static function setFPS(fps:String):void
		{
			var rate:Number = parseInt(fps);
			if (StageRef != null && rate > 0)
			{
				StageRef.frameRate = rate;
			}
		}

		public static function sendMessage(msg:String):void
		{
			try
			{
				if (Game != null && Game.chatF != null)
				{
					Game.chatF.pushMsg("server", msg, "SERVER", "", 0);
				}
			}
			catch (e:Error) {}
		}
	}
}
