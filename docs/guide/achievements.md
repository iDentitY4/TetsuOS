# RetroAchievements

[RetroAchievements](https://retroachievements.org) adds achievements to classic games.
Tetsu OS supports it on every console except the Tamagotchi, through rcheevos, the same
library RetroArch uses.

## Logging in

You need a free account from retroachievements.org and a Wi-Fi connection.

1. Open **Settings > IN GAME > ACHIEVEMENTS**.
2. Choose **ACCOUNT** and type your user name, then your password, on the keyboard.

Only your user name and a login token are kept on the card, never the password. Choose
ACCOUNT again to log out.

## During a game

When a game starts, a notice shows how many of its achievements you have. Unlocks show
with their badge and points, and progress notices appear at 25, 50 and 75% of the way.

**ACHIEVEMENTS** in the in-game menu opens the list of the game's achievements, grouped as
almost there, locked and unlocked, with badges and progress such as `73/100`.

![The achievements page](/screenshots/achievements.png)

A game's details screen shows your count, such as `3 OF 40`.

## Notices

Each kind of notice (UNLOCKS, PROGRESS and OTHER MESSAGES) can show at the BOTTOM (the
default), the TOP, or be HIDDEN. Set them for every game in **Settings > IN GAME >
ACHIEVEMENTS**, or for one game in the in-game menu's ACHIEVEMENTS list. Hidden notices
still unlock.

## Turning them off

- **For every game:** the first row of Settings > IN GAME > ACHIEVEMENTS. The account is
  kept.
- **For one game:** the ACHIEVEMENTS row on the game's details (Y).

Both apply from the game's next start.

## Hardcore mode

HARDCORE counts unlocks as hardcore, with the rules of the original hardware: the game
starts from its save rather than a resume point, and loading states, cheats and
fast-forward are off. Saving states still works. Core options that RetroAchievements
doesn't allow in hardcore, such as a PAL region, switch the game to softcore with a notice.

::: warning Not available yet
The RetroAchievements server refuses hardcore unlocks from emulators it doesn't know.
Tetsu OS still has to be registered there; until then, hardcore is refused and the server
adds a "Warning: unknown emulator" achievement.
:::

## Playing offline

A game you have played once online keeps working offline: its achievements load from the
copy kept on the card, and unlocks wait in a list. They are sent, dated when they happened,
the next time the device is online, from the menus or when a game starts. Game completed
and mastered notices, leaderboards and your score need a connection.

## Limits

- `.chd` PlayStation games are not recognised, as rcheevos can't read them. Use `.cue` and
  `.bin`.
- Leaderboard trackers and challenge indicators aren't shown.
