# Django Testing with pytest

## Setup

### pytest.ini

```ini
[pytest]
DJANGO_SETTINGS_MODULE = config.settings.test
testpaths = tests
python_files = test_*.py
python_classes = Test*
python_functions = test_*
addopts =
    --reuse-db
    --nomigrations
    --cov=apps
    --cov-report=html
    --cov-report=term-missing
    --strict-markers
markers =
    slow: marks tests as slow
    integration: marks tests as integration tests
```

### Test Settings

```python
# config/settings/test.py
from .base import *

DEBUG = True
DATABASES = {
    'default': {
        'ENGINE': 'django.db.backends.sqlite3',
        'NAME': ':memory:',
    }
}

# Disable migrations for speed
class DisableMigrations:
    def __contains__(self, item): return True
    def __getitem__(self, item): return None

MIGRATION_MODULES = DisableMigrations()

# Faster password hashing
PASSWORD_HASHERS = ['django.contrib.auth.hashers.MD5PasswordHasher']

EMAIL_BACKEND = 'django.core.mail.backends.console.EmailBackend'

# Celery always eager
CELERY_TASK_ALWAYS_EAGER = True
CELERY_TASK_EAGER_PROPAGATES = True
```

### conftest.py

```python
# tests/conftest.py
import pytest
from django.contrib.auth import get_user_model
from rest_framework.test import APIClient

User = get_user_model()

@pytest.fixture
def user(db):
    return User.objects.create_user(
        email='test@example.com', password='testpass123', username='testuser'
    )

@pytest.fixture
def admin_user(db):
    return User.objects.create_superuser(
        email='admin@example.com', password='adminpass123', username='admin'
    )

@pytest.fixture
def authenticated_client(client, user):
    client.force_login(user)
    return client

@pytest.fixture
def api_client():
    return APIClient()

@pytest.fixture
def authenticated_api_client(api_client, user):
    api_client.force_authenticate(user=user)
    return api_client
```

## Factory Boy

```python
# tests/factories.py
import factory
from factory import fuzzy
from django.contrib.auth import get_user_model
from apps.products.models import Product, Category

User = get_user_model()

class UserFactory(factory.django.DjangoModelFactory):
    class Meta:
        model = User

    email = factory.Sequence(lambda n: f"user{n}@example.com")
    username = factory.Sequence(lambda n: f"user{n}")
    password = factory.PostGenerationMethodCall('set_password', 'testpass123')
    first_name = factory.Faker('first_name')
    last_name = factory.Faker('last_name')
    is_active = True

class CategoryFactory(factory.django.DjangoModelFactory):
    class Meta:
        model = Category

    name = factory.Faker('word')
    slug = factory.LazyAttribute(lambda obj: obj.name.lower())

class ProductFactory(factory.django.DjangoModelFactory):
    class Meta:
        model = Product

    name = factory.Faker('sentence', nb_words=3)
    slug = factory.LazyAttribute(lambda obj: obj.name.lower().replace(' ', '-'))
    price = fuzzy.FuzzyDecimal(10.00, 1000.00, 2)
    stock = fuzzy.FuzzyInteger(0, 100)
    is_active = True
    category = factory.SubFactory(CategoryFactory)
    created_by = factory.SubFactory(UserFactory)

    @factory.post_generation
    def tags(self, create, extracted, **kwargs):
        if not create:
            return
        if extracted:
            for tag in extracted:
                self.tags.add(tag)
```

## Model Tests

```python
class TestProductModel:
    def test_product_creation(self, db):
        product = ProductFactory(price=100.00, stock=50)
        assert product.price == 100.00
        assert product.is_active is True

    def test_product_slug_generation(self, db):
        product = ProductFactory(name='Test Product')
        assert product.slug == 'test-product'

    def test_product_price_validation(self, db):
        product = ProductFactory(price=-10)
        with pytest.raises(ValidationError):
            product.full_clean()

    def test_active_queryset(self, db):
        ProductFactory.create_batch(5, is_active=True)
        ProductFactory.create_batch(3, is_active=False)
        assert Product.objects.active().count() == 5
```

## API Tests

```python
class TestProductAPI:
    def test_list_products(self, api_client, db):
        ProductFactory.create_batch(10)
        response = api_client.get(reverse('api:product-list'))
        assert response.status_code == 200
        assert response.data['count'] == 10

    def test_create_unauthorized(self, api_client, db):
        response = api_client.post(reverse('api:product-list'), {'name': 'Test'})
        assert response.status_code == 401

    def test_create_authorized(self, authenticated_api_client, db):
        data = {'name': 'Test', 'price': '99.99', 'stock': 10}
        response = authenticated_api_client.post(reverse('api:product-list'), data)
        assert response.status_code == 201

    def test_filter_by_price(self, api_client, db):
        ProductFactory(price=50)
        ProductFactory(price=150)
        response = api_client.get(reverse('api:product-list'), {'price_min': 100})
        assert response.data['count'] == 1
```

## Mocking External Services

```python
from unittest.mock import patch

class TestPaymentView:
    @patch('apps.payments.services.stripe')
    def test_successful_payment(self, mock_stripe, client, user, product):
        mock_stripe.Charge.create.return_value = {
            'id': 'ch_123', 'status': 'succeeded', 'amount': 9999
        }
        client.force_login(user)
        response = client.post(reverse('payments:process'), {
            'product_id': product.id, 'token': 'tok_visa',
        })
        assert response.status_code == 302
        mock_stripe.Charge.create.assert_called_once()

    @patch('apps.payments.services.stripe')
    def test_failed_payment(self, mock_stripe, client, user, product):
        mock_stripe.Charge.create.side_effect = Exception('Card declined')
        client.force_login(user)
        response = client.post(reverse('payments:process'), {
            'product_id': product.id, 'token': 'tok_visa',
        })
        assert 'error' in response.url
```

## Email Testing

```python
from django.core import mail
from django.test import override_settings

@override_settings(EMAIL_BACKEND='django.core.mail.backends.locmem.EmailBackend')
def test_order_confirmation_email(db, order):
    order.send_confirmation_email()
    assert len(mail.outbox) == 1
    assert order.user.email in mail.outbox[0].to
```

## Integration Tests

```python
class TestCheckoutFlow:
    def test_guest_to_purchase_flow(self, client, db):
        # Register
        client.post(reverse('users:register'), {
            'email': 'test@example.com',
            'password': 'testpass123', 'password_confirm': 'testpass123',
        })
        # Login
        client.post(reverse('users:login'), {
            'email': 'test@example.com', 'password': 'testpass123',
        })
        # Add to cart
        product = ProductFactory(price=100)
        client.post(reverse('cart:add'), {
            'product_id': product.id, 'quantity': 1,
        })
        # Checkout
        with patch('apps.checkout.services.process_payment') as mock:
            mock.return_value = True
            response = client.post(reverse('checkout:complete'))
        assert Order.objects.filter(user__email='test@example.com').exists()
```

## Best Practices

**DO:**
- Use factories, not manual object creation
- One assertion focus per test
- Descriptive names: `test_user_cannot_delete_others_post`
- Mock external services only (Stripe, email, S3)
- Use `--reuse-db` and `--nomigrations` for speed

**DON'T:**
- Don't test Django internals
- Don't test third-party library code
- Don't make tests order-dependent
- Don't over-mock (mock only external dependencies)
- Don't test private methods

## Coverage Targets

| Component | Target |
|-----------|--------|
| Models | 90%+ |
| Serializers | 85%+ |
| Views | 80%+ |
| Services | 90%+ |
| Overall | 80%+ |
