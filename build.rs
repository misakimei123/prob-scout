fn main() {
    // SQLx 会把 migration 嵌入 binary；目录新增文件时必须触发重新编译。
    println!("cargo:rerun-if-changed=migrations");
}
