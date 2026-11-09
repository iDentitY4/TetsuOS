/* The part of libretro.h that rc_libretro.c uses: memory types and the memory map a
 * core gives through RETRO_ENVIRONMENT_SET_MEMORY_MAPS. Copied from libretro-common's
 * libretro.h (MIT, Copyright (C) 2010-2024 The RetroArch team), which has the rest. */
#ifndef LIBRETRO_H__
#define LIBRETRO_H__

#include <stddef.h>
#include <stdint.h>

#define RETRO_MEMORY_SAVE_RAM    0
#define RETRO_MEMORY_RTC         1
#define RETRO_MEMORY_SYSTEM_RAM  2
#define RETRO_MEMORY_VIDEO_RAM   3

struct retro_memory_descriptor
{
   uint64_t flags;
   void *ptr;
   size_t offset;
   size_t start;
   size_t select;
   size_t disconnect;
   size_t len;
   const char *addrspace;
};

struct retro_memory_map
{
   const struct retro_memory_descriptor *descriptors;
   unsigned num_descriptors;
};

#endif
