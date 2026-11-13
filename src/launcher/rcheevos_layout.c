/* The sizes of the rcheevos structs that achievements.rs declares by hand, for the test
 * there that compares them. Nothing else uses this file, so the linker leaves it out of
 * the launcher. */
#include <stddef.h>
#include "rc_client.h"
#include "rc_api_request.h"
#include "rc_api_runtime.h"
#include "rcheevos/src/rc_libretro.h"

const size_t launcher_sizeof_rc_api_request_t = sizeof(rc_api_request_t);
const size_t launcher_sizeof_rc_api_server_response_t = sizeof(rc_api_server_response_t);
const size_t launcher_sizeof_rc_client_user_t = sizeof(rc_client_user_t);
const size_t launcher_sizeof_rc_client_game_t = sizeof(rc_client_game_t);
const size_t launcher_sizeof_rc_client_user_game_summary_t = sizeof(rc_client_user_game_summary_t);
const size_t launcher_sizeof_rc_client_achievement_t = sizeof(rc_client_achievement_t);
const size_t launcher_sizeof_rc_client_achievement_bucket_t = sizeof(rc_client_achievement_bucket_t);
const size_t launcher_sizeof_rc_client_achievement_list_t = sizeof(rc_client_achievement_list_t);
const size_t launcher_sizeof_rc_client_leaderboard_t = sizeof(rc_client_leaderboard_t);
const size_t launcher_sizeof_rc_client_server_error_t = sizeof(rc_client_server_error_t);
const size_t launcher_sizeof_rc_client_event_t = sizeof(rc_client_event_t);
const size_t launcher_sizeof_rc_libretro_memory_regions_t = sizeof(rc_libretro_memory_regions_t);
const size_t launcher_sizeof_rc_libretro_core_memory_info_t = sizeof(rc_libretro_core_memory_info_t);
const size_t launcher_sizeof_retro_memory_descriptor = sizeof(struct retro_memory_descriptor);
const size_t launcher_sizeof_retro_memory_map = sizeof(struct retro_memory_map);
const size_t launcher_sizeof_rc_api_award_achievement_request_t = sizeof(rc_api_award_achievement_request_t);
