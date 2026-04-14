CREATE TABLE user_food_preferences (
  user_id UUID REFERENCES users(id) ON DELETE CASCADE,
  preference TEXT NOT NULL,
  PRIMARY KEY (user_id, preference)
);
