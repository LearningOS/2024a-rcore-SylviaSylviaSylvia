# 总结
  实现了sys_spawn和sys_set_priority系统调用，并在sys_set_priority系统调用基础上实现了stride 调度算法。sys_spawn使得进程在创建时不需要复制父进程的地址空间，直接根据即将执行的新进程的情况创建，减少不必要的工作。sys_set_priority可以为各个进程设置优先级。

# 问答题
  stride 算法原理非常简单，但是有一个比较大的问题。例如两个 pass = 10 的进程，使用 8bit 无符号整形储存 stride， p1.stride = 255, p2.stride = 250，在 p2 执行一个时间片后，理论上下一次应该 p1 执行。
  * 实际情况是轮到 p1 执行吗？为什么？  
    不是。是轮到p2执行。
    因为stride 是 8bit 无符号整数，p2.stride 在增加其 pass 值后会发生溢出，变为 4，比p1.stride小，因而调度器会选择p2。

  我们之前要求进程优先级 >= 2 其实就是为了解决这个问题。可以证明， 在不考虑溢出的情况下 , 在进程优先级全部 >= 2 的情况下，如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。
  * 为什么？尝试简单说明（不要求严格证明）。  
    设STRIDE_MAX对应的进程为p1，STRIDE_MIN对应的进程为p2，p1.pass = BigStride / p1.prio，p2.pass = BigStride / p2.prio。  
    p1.pass - p2.pass = BigStride / p1.prio - BigStride / p2.prio = BigStride * ((1 / p1.prio) - (1 / p2.prio))  因为进程优先级 >=2，所以 (1 / p1.prio) 和 (1 / p2.prio) 中较大的那个最大为 1/2 ，减去一个大于零的数后一定不会大于 1/2。  
    所以 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。  

  * 已知以上结论，考虑溢出的情况下，可以为 Stride 设计特别的比较器，让 BinaryHeap<Stride> 的 pop 方法能返回真正最小的 Stride。补全下列代码中的 partial_cmp 函数，假设两个 Stride 永远不会相等。
  ```
  use core::cmp::Ordering;

  struct Stride(u64);

  impl PartialOrd for Stride {
      fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
          let difference = self.0.wrapping_sub(other.0);
          if difference < BIG_STRIDE / 2 {
              Some(Ordering::Less)
          } else {
              Some(Ordering::Greater)
          }
      }
  }

  impl PartialEq for Stride {
      fn eq(&self, other: &Self) -> bool {
          false
      }
  }
  ```
  TIPS: 使用 8 bits 存储 stride, BigStride = 255, 则: (125 < 255) == false, (129 < 255) == true.

# 荣誉准则 
1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：  
   群友 环戊烷, My

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：  
   rCore-Camp-Guide-2024A 文档, 训练营课程，rCore-Tutorial-Book-v3 文档

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。