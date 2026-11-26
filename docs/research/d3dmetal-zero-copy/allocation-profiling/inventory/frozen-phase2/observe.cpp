// SPDX-License-Identifier: GPL-3.0-or-later
// Original macOS process accounting observer; native ARM64 only.
#include <libproc.h>
#include <sys/resource.h>
#include <cstdio>
#include <cstdlib>
#include <cerrno>
#include <unistd.h>
int main(int argc,char** argv){
    if(argc!=2)return 2;
    rusage_info_v4 r{};int pid=std::atoi(argv[1]);
    int rc=proc_pid_rusage(pid,RUSAGE_INFO_V4,reinterpret_cast<rusage_info_t*>(&r));
    std::printf("{\"pid\":%d,\"return\":%d,\"errno\":%d,\"host_page_size\":%d,\"phys_footprint\":%llu,\"resident_size\":%llu,\"wired_size\":%llu,\"pageins\":%llu,\"max_phys_footprint\":%llu}\n",pid,rc,rc?errno:0,getpagesize(),r.ri_phys_footprint,r.ri_resident_size,r.ri_wired_size,r.ri_pageins,r.ri_lifetime_max_phys_footprint);
    return rc?1:0;
}
