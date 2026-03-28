# Django ORM Patterns

## Model Best Practices

```python
from django.db import models
from django.contrib.auth.models import AbstractUser
from django.core.validators import MinValueValidator

class User(AbstractUser):
    email = models.EmailField(unique=True)
    phone = models.CharField(max_length=20, blank=True)

    USERNAME_FIELD = 'email'
    REQUIRED_FIELDS = ['username']

    class Meta:
        db_table = 'users'
        ordering = ['-date_joined']

    def __str__(self):
        return self.email

class Product(models.Model):
    name = models.CharField(max_length=200)
    slug = models.SlugField(unique=True, max_length=250)
    description = models.TextField(blank=True)
    price = models.DecimalField(
        max_digits=10, decimal_places=2,
        validators=[MinValueValidator(0)]
    )
    stock = models.PositiveIntegerField(default=0)
    is_active = models.BooleanField(default=True)
    category = models.ForeignKey('Category', on_delete=models.CASCADE, related_name='products')
    tags = models.ManyToManyField('Tag', blank=True, related_name='products')
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        db_table = 'products'
        ordering = ['-created_at']
        indexes = [
            models.Index(fields=['slug']),
            models.Index(fields=['-created_at']),
            models.Index(fields=['category', 'is_active']),
        ]
        constraints = [
            models.CheckConstraint(
                check=models.Q(price__gte=0),
                name='price_non_negative'
            )
        ]

    def __str__(self):
        return self.name

    def save(self, *args, **kwargs):
        if not self.slug:
            self.slug = slugify(self.name)
        super().save(*args, **kwargs)
```

## Custom QuerySet

Chainable query methods — the core ORM pattern.

```python
class ProductQuerySet(models.QuerySet):
    def active(self):
        return self.filter(is_active=True)

    def with_category(self):
        return self.select_related('category')

    def with_tags(self):
        return self.prefetch_related('tags')

    def in_stock(self):
        return self.filter(stock__gt=0)

    def search(self, query):
        return self.filter(
            models.Q(name__icontains=query) |
            models.Q(description__icontains=query)
        )

class Product(models.Model):
    # ... fields ...
    objects = ProductQuerySet.as_manager()

# Usage: Product.objects.active().with_category().in_stock()
```

## Custom Manager

```python
class ProductManager(models.Manager):
    def get_or_none(self, **kwargs):
        try:
            return self.get(**kwargs)
        except self.model.DoesNotExist:
            return None

    def create_with_tags(self, name, price, tag_names):
        product = self.create(name=name, price=price)
        tags = [Tag.objects.get_or_create(name=n)[0] for n in tag_names]
        product.tags.set(tags)
        return product

    def bulk_update_stock(self, product_ids, quantity):
        return self.filter(id__in=product_ids).update(stock=quantity)
```

## N+1 Query Prevention

```python
# BAD — N+1: separate query for each product's category
products = Product.objects.all()
for p in products:
    print(p.category.name)  # N extra queries

# GOOD — select_related for ForeignKey (JOIN)
products = Product.objects.select_related('category').all()

# GOOD — prefetch_related for ManyToMany (2 queries total)
products = Product.objects.prefetch_related('tags').all()
```

## Bulk Operations

```python
# Bulk create (single INSERT)
Product.objects.bulk_create([
    Product(name=f'Product {i}', price=10.00)
    for i in range(1000)
])

# Bulk update
products = Product.objects.all()[:100]
for p in products:
    p.is_active = True
Product.objects.bulk_update(products, ['is_active'])
```

## Database Indexing

```python
class Meta:
    indexes = [
        models.Index(fields=['name']),
        models.Index(fields=['-created_at']),
        models.Index(fields=['category', 'created_at']),  # Composite
    ]
```
