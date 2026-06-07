#include "page.hpp"
#include "page_alloc.hpp"

constinit inline static PageManager<4> ttb0_manager{};
constinit inline static PageManager<4> ttb1_manager{};

constinit inline static PageAllocator page_allocator{};

constinit inline static PageManager<512> *l2_page_manager = nullptr;
