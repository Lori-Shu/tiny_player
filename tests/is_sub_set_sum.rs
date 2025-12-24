#[test]
fn test_sub_set_sum() {
    // 3, 34, 4, 12, 5, 2 sum 9
    // 1, 2, 3 sum 6
    // 5 ,10 ,12 ,13 ,15 ,18  sum 30
    // 4,8,5 sum 9
    let arr = vec![5, 10, 12, 13, 15, 18];
    let sum = 30;
    let mut ans_arr = vec![];
    println!(
        "result:{},chose indices{:?}",
        is_sub_set_sum_recursion(&arr, &sum, &mut ans_arr, arr.iter().sum::<i32>(), 0, 0),
        ans_arr
    );
    // println!("result:{}", is_sub_set_sum(&arr, &sum));
    // println!("result:{}", is_sub_set_sum_dp(&arr, &sum));
}
fn is_sub_set_sum_recursion(
    arr: &[i32],
    sum: &i32,
    ans_arr: &mut Vec<usize>,
    left_elements_sum: i32,
    temp_sum: i32,
    index: usize,
) -> bool {
    if temp_sum > *sum {
        return false;
    }
    if temp_sum == *sum {
        return true;
    }
    if index >= arr.len() {
        return false;
    }
    let upper_bound = temp_sum + left_elements_sum;
    if upper_bound < *sum {
        return false;
    }
    ans_arr.push(index);
    let result_chose = is_sub_set_sum_recursion(
        arr,
        sum,
        ans_arr,
        left_elements_sum - arr[index],
        temp_sum + arr[index],
        index + 1,
    );
    if !result_chose {
        ans_arr.remove(ans_arr.len() - 1);
        let result_skip = is_sub_set_sum_recursion(
            arr,
            sum,
            ans_arr,
            left_elements_sum - arr[index],
            temp_sum,
            index + 1,
        );
        return result_skip;
    }
    result_chose
}
