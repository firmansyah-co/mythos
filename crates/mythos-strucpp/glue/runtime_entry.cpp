#include <cstdint>
#include <cstddef>
#include <pthread.h>
#include "/home/adrian/Projects/openplc-editor/node_modules/strucpp/src/runtime/include/debug_table.hpp"

namespace strucpp {
namespace debug {
    const uint8_t debug_array_count = 0;
    const uint16_t debug_array_counts[1] = { 0 };
    const Entry* const debug_arrays[1] = { nullptr };
    const RetainVar retain_vars[1] = { { 0, 0 } };
    const uint16_t retain_var_count = 0;
    const uint32_t retain_layout_hash = 0;
}
}

#define STRUCPP_V4_DEBUG_EXPORTS_DEFINE
#include "/home/adrian/Projects/openplc-editor/node_modules/strucpp/src/runtime/include/debug_dispatch.hpp"
#include "generated.hpp"

namespace strucpp {
    pthread_mutex_t* g_image_tables_mutex_ptr = nullptr;
    pthread_mutex_t* g_global_vars_mutex_ptr  = nullptr;
}

static strucpp::Configuration_CONFIG0 g_config;

extern "C" strucpp::ConfigurationInstance* strucpp_get_config(void) {
    return &g_config;
}

extern "C" void strucpp_set_locks(pthread_mutex_t* image_tables_mutex,
                                  pthread_mutex_t* global_vars_mutex) {
    strucpp::g_image_tables_mutex_ptr = image_tables_mutex;
    strucpp::g_global_vars_mutex_ptr  = global_vars_mutex;
}

extern "C" strucpp::LocatedVar* strucpp_get_located_vars(uint32_t* count) {
    if (count) {
        *count = strucpp::locatedVarsCount;
    }
    return strucpp::locatedVars;
}

extern "C" uint32_t strucpp_get_located_vars_count(void) {
    return strucpp::locatedVarsCount;
}

extern "C" void strucpp_step_cycle(void) {
    size_t r_cnt = g_config.get_resource_count();
    strucpp::ResourceInstance* resources = g_config.get_resources();
    for (size_t r = 0; r < r_cnt; ++r) {
        for (size_t t = 0; t < resources[r].task_count; ++t) {
            strucpp::TaskInstance& task = resources[r].tasks[t];
            for (size_t p = 0; p < task.program_count; ++p) {
                task.programs[p]->sync_in();
                task.programs[p]->run();
                task.programs[p]->sync_out();
            }
        }
    }
}
