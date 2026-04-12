mod common;

#[tokio::test]
async fn test_infrastructure_works() {
    let ctx = common::TestContext::new().await;

    // Verify we can query the database
    let result = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(result, 1);
}
