package collection

// held gives c copies in one finish and condition (test fixtures).
func held(c Card, finish string, cond Condition, l Purchase) Card {
	c.AddPurchase(finish, cond, l)
	return c
}
